use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::backend::mesh::{self as mesh_backend, MeshConnectionStatus};
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::util::{
    format_bytes, format_download_size, format_tps, model_display_name, thousands,
};
use crate::ui::welcome::first_model;
use crate::ui::widgets::{self, Tone};
use crate::ui::{Navigate, Route};

pub const EARLY_MESSAGE: &str =
    "You're early — no peers online yet. Your machine will help others as the hive grows.";

pub struct Dashboard {
    state: Arc<AppState>,
    shared: Entity<Shared>,
    _observe: Subscription,
}

impl EventEmitter<Navigate> for Dashboard {}

impl Dashboard {
    pub fn new(
        state: Arc<AppState>,
        shared: Entity<Shared>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let _observe = cx.observe(&shared, |_, _, cx| cx.notify());
        Self {
            state,
            shared,
            _observe,
        }
    }

    pub fn enter(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.shared.update(cx, |s, cx| {
            s.refresh_loaded(cx);
            s.refresh_installed(cx);
        });
    }

    /// The top card: a running download, first-run install, the loaded
    /// model, or the installed models to load.
    fn hero(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shared = self.shared.read(cx);
        if let Some(job) = &shared.install {
            return Some(
                widgets::download_progress(&job.model_id, job.progress.as_ref()).into_any_element(),
            );
        }

        let has_models = !shared.installed.is_empty();
        let lists_loading = !shared.installed_loaded || shared.recommendations_loading;

        if !has_models && !lists_loading {
            // Never an empty first-run card: fall back to a tiny model if the
            // recommender found nothing.
            let (top, is_fallback) =
                first_model(shared.recommendations.as_deref().unwrap_or_default());
            let size = format_download_size(top.estimated_download_bytes);
            let label = format!(
                "Install {} ({}{})",
                top.model_name,
                top.quantization,
                size.as_ref().map(|s| format!(", {s}")).unwrap_or_default()
            );
            let mut detail = size.map(|s| format!("{s} download · ")).unwrap_or_default();
            detail.push_str(&format!(
                "{} RAM",
                format_bytes(top.estimated_memory_usage_bytes)
            ));
            if top.estimated_tokens_per_sec > 0.0 {
                detail.push_str(&format!(" · {}", format_tps(top.estimated_tokens_per_sec)));
            }
            let (id, quant) = (top.model_id.clone(), top.quantization.to_string());
            return Some(
                widgets::accent_card()
                    .child(
                        h_flex()
                            .items_start()
                            .gap_4()
                            .child(widgets::icon_tile(IconName::Zap, 48., ca(PAW_500, 0.1), c(PAW_500)))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Get Started"))
                                    .child(widgets::secondary(if is_fallback {
                                        "Start with a small, fast model that runs on almost any machine. You can add bigger ones from the Model Browser."
                                    } else {
                                        "HiveBear analyzed your hardware and picked a model that runs well here. One click to install and start chatting."
                                    }))
                                    .child(
                                        h_flex()
                                            .pt_3()
                                            .gap_4()
                                            .child(
                                                widgets::button("install-top", label)
                                                    .icon(IconName::Download)
                                                    .primary()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        let (id, quant) = (id.clone(), quant.clone());
                                                        this.shared.update(cx, |s, cx| s.install(&id, Some(quant), cx));
                                                    })),
                                            )
                                            .child(widgets::muted(detail)),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        }

        if let Some(first) = shared.loaded.first() {
            let mut detail = format!("Ready · ctx {}", thousands(first.context_length.into()));
            if let Some(layers) = first.gpu_layers {
                detail.push_str(&format!(" · {layers} GPU layers"));
            }
            let others: Vec<String> = shared
                .loaded
                .iter()
                .skip(1)
                .map(|m| model_display_name(&m.model_path))
                .collect();
            return Some(
                widgets::accent_card()
                    .child(
                        h_flex()
                            .gap_3()
                            .child(widgets::icon_tile(
                                IconName::Bot,
                                40.,
                                ca(SUCCESS, 0.1),
                                c(SUCCESS),
                            ))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(model_display_name(&first.model_path)),
                                            )
                                            .child(widgets::badge(
                                                first.engine.to_string(),
                                                Tone::Default,
                                            )),
                                    )
                                    .child(widgets::muted(detail)),
                            )
                            .child(
                                widgets::button("open-chat", "Open Chat")
                                    .icon(IconName::MessageSquare)
                                    .primary()
                                    .on_click(
                                        cx.listener(|_, _, _, cx| cx.emit(Navigate(Route::Chat))),
                                    ),
                            ),
                    )
                    .when(!others.is_empty(), |d| {
                        d.child(
                            h_flex()
                                .mt_3()
                                .pt_3()
                                .gap_2()
                                .border_t_1()
                                .border_color(c(BORDER))
                                .children(
                                    others
                                        .into_iter()
                                        .map(|n| widgets::badge(format!("● {n}"), Tone::Accent)),
                                ),
                        )
                    })
                    .into_any_element(),
            );
        }

        if has_models {
            let count = shared.installed.len();
            let busy = shared.loading_model.clone();
            let models: Vec<(String, String)> = shared
                .installed
                .iter()
                .take(3)
                .map(|m| (m.id.clone(), m.name.clone()))
                .collect();
            return Some(
                widgets::accent_card()
                    .child(
                        h_flex()
                            .gap_3()
                            .child(
                                Icon::new(IconName::MessageSquare)
                                    .size(px(20.))
                                    .text_color(c(PAW_500)),
                            )
                            .child(
                                v_flex()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Ready to chat"),
                                    )
                                    .child(div().text_xs().text_color(c(TEXT_SECONDARY)).child(
                                        format!(
                                            "{count} model{} installed. Load one to start.",
                                            if count > 1 { "s" } else { "" }
                                        ),
                                    )),
                            ),
                    )
                    .child(h_flex().mt_3().gap_2().children(models.into_iter().map(
                        |(id, name)| {
                            let loading = busy.as_deref() == Some(id.as_str());
                            widgets::button(SharedString::from(format!("load-{id}")), name)
                                .icon(IconName::Zap)
                                .outline()
                                .small()
                                .loading(loading)
                                .disabled(busy.is_some())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let id = id.clone();
                                    this.shared.update(cx, |s, cx| s.load_model(&id, true, cx));
                                }))
                        },
                    )))
                    .into_any_element(),
            );
        }
        None
    }
}

impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let profile = self.state.profile.clone();
        let status = mesh_backend::connection_status(&self.state);
        let hero = self.hero(cx);
        let pill = mesh_pill(&status, cx);
        let shared = self.shared.read(cx);

        let header = h_flex()
            .justify_between()
            .child(widgets::page_title("Dashboard"))
            .child(pill);

        // ── System ──────────────────────────────────────────────────
        let mut gauges = v_flex()
            .flex_grow(3.)
            .flex_basis(px(0.))
            .gap_4()
            .child(widgets::gauge(
                "Memory Usage",
                profile
                    .memory
                    .total_bytes
                    .saturating_sub(profile.memory.available_bytes),
                profile.memory.total_bytes,
            ));
        if let Some(gpu) = profile.gpus.first() {
            gauges = gauges.child(widgets::gauge("GPU VRAM", 0, gpu.vram_bytes));
        }
        let cpu = &profile.cpu;
        let gpu_row = match profile.gpus.first() {
            Some(gpu) => hardware_row(
                IconName::Monitor,
                "GPU",
                gpu.name.clone(),
                format!("{} · {}", format_bytes(gpu.vram_bytes), gpu.compute_api),
            ),
            None => hardware_row(
                IconName::Monitor,
                "GPU",
                "None detected".into(),
                "CPU-only inference".into(),
            ),
        };
        let hardware = div().flex_grow(2.).flex_basis(px(0.)).child(
            widgets::card().child(
                v_flex()
                    .gap_3()
                    .child(widgets::overline("Hardware"))
                    .child(hardware_row(
                        IconName::Cpu,
                        "CPU",
                        cpu.model_name.clone(),
                        format!("{}c / {}t", cpu.physical_cores, cpu.logical_cores),
                    ))
                    .child(hardware_row(
                        IconName::MemoryStick,
                        "RAM",
                        format_bytes(profile.memory.total_bytes),
                        format!("{:.1} GB/s", profile.memory.estimated_bandwidth_gbps),
                    ))
                    .child(gpu_row)
                    .child(hardware_row(
                        IconName::HardDrive,
                        "Storage",
                        format!("{} free", format_bytes(profile.storage.available_bytes)),
                        format!("{:.0} MB/s read", profile.storage.estimated_read_speed_mbps),
                    )),
            ),
        );
        let system = h_flex().items_start().gap_4().child(gauges).child(hardware);

        // ── Discovery ───────────────────────────────────────────────
        let mut discovery = v_flex().gap_2().child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(TEXT_SECONDARY))
                .child("Recommended for This Device"),
        );
        let recs = shared.recommendations.clone();
        let has_models = !shared.installed.is_empty();
        match recs.as_deref() {
            None => discovery = discovery.child(widgets::muted("Calculating...")),
            Some([]) => {
                discovery =
                    discovery.child(widgets::muted("No models found for this hardware profile."))
            }
            Some(recs) => {
                for rec in recs.iter().take(3) {
                    let tps = rec.estimated_tokens_per_sec;
                    let speed = if tps >= 20.0 {
                        SUCCESS
                    } else if tps >= 10.0 {
                        WARNING
                    } else {
                        DANGER
                    };
                    discovery = discovery.child(
                        widgets::clickable_card(SharedString::from(format!(
                            "rec-{}-{}",
                            rec.model_id, rec.quantization
                        )))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(rec.model_name.clone()),
                                )
                                .child(widgets::badge(rec.quantization.to_string(), Tone::Default))
                                .child(div().flex_1())
                                .children(format_download_size(rec.estimated_download_bytes).map(
                                    |s| div().text_xs().text_color(c(TEXT_SECONDARY)).child(s),
                                ))
                                .child(widgets::muted(format!(
                                    "{} RAM",
                                    format_bytes(rec.estimated_memory_usage_bytes)
                                )))
                                .child(div().text_xs().text_color(c(speed)).child(format_tps(tps)))
                                .child(widgets::mono(
                                    format!("{:.0}%", rec.confidence * 100.0),
                                    TEXT_MUTED,
                                )),
                        )
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Navigate(Route::Models)))),
                    );
                }
            }
        }

        let page = v_flex()
            .gap_6()
            .child(header)
            .children(hero)
            .child(system)
            .child(discovery)
            .when(has_models, |d| {
                d.child(
                    widgets::clickable_card("mesh-callout")
                        .child(
                            h_flex()
                                .gap_3()
                                .child(Icon::new(IconName::Users).size(px(14.)).text_color(c(PAW_500)))
                                .child(div().text_xs().text_color(c(TEXT_SECONDARY)).child(
                                    "Share your idle compute with the hive. Help other bears run AI on devices that can't do it alone.",
                                )),
                        )
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Navigate(Route::Mesh)))),
                )
            });
        widgets::scroll_page("dashboard", page)
    }
}

fn hardware_row(icon: IconName, title: &'static str, value: String, detail: String) -> Div {
    h_flex()
        .items_start()
        .gap_2p5()
        .child(
            div()
                .pt_0p5()
                .child(Icon::new(icon).size(px(14.)).text_color(c(TEXT_MUTED))),
        )
        .child(
            v_flex()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(c(TEXT_MUTED))
                        .child(title.to_uppercase()),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(value),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(c(TEXT_MUTED))
                        .truncate()
                        .child(detail),
                ),
        )
}

/// Compact mesh-status pill for the dashboard header.
///
/// This is the retention lever: users need to see that the mesh is running
/// and that they are connected to others.
fn mesh_pill(status: &MeshConnectionStatus, cx: &mut Context<Dashboard>) -> Stateful<Div> {
    // Running locally but unregistered is not "connecting" — nothing will
    // change until the coordination server comes back. Registered with
    // nobody else online is a settled state, not a pending one.
    let (label, icon, color) = if !status.running {
        ("Mesh idle".to_string(), IconName::Network, TEXT_MUTED)
    } else if !status.registered {
        ("Mesh offline".to_string(), IconName::WifiOff, WARNING)
    } else if status.peer_count == 0 {
        (
            "You're early · no peers yet".to_string(),
            IconName::Network,
            PAW_400,
        )
    } else {
        let n = status.peer_count;
        (
            format!("Mesh active · {n} peer{}", if n == 1 { "" } else { "s" }),
            IconName::Users,
            SUCCESS,
        )
    };
    let muted = color == TEXT_MUTED;
    h_flex()
        .id("mesh-pill")
        .gap_2()
        .px_3()
        .py_1p5()
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .text_color(c(color))
        .bg(if muted {
            c(SURFACE_OVERLAY)
        } else {
            ca(color, 0.1)
        })
        .border_1()
        .border_color(if muted { c(BORDER) } else { ca(color, 0.3) })
        .child(Icon::new(icon).size(px(12.)))
        .child(label)
        .on_click(cx.listener(|_, _, _, cx| cx.emit(Navigate(Route::Mesh))))
}
