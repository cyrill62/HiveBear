use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Element, FillPortion, Task};

use crate::backend::mesh::{self as mesh_backend, MeshConnectionStatus};
use crate::ui::icon::{self, icon};
use crate::ui::theme::*;
use crate::ui::util::{
    format_bytes, format_download_size, format_tps, model_display_name, thousands,
};
use crate::ui::welcome::first_model;
use crate::ui::widgets::{self, Kind, Pad, Tone};
use crate::ui::{scroll_page, Cx, Env, Route};

pub const EARLY_MESSAGE: &str =
    "You're early — no peers online yet. Your machine will help others as the hive grows.";

#[derive(Debug, Clone)]
pub enum Msg {
    Go(Route),
    Install { model_id: String, quant: String },
    Load(String),
}

#[derive(Default)]
pub struct Dashboard;

impl Dashboard {
    pub fn enter(&mut self, env: &mut Env) -> Task<Msg> {
        env.shared.refresh_loaded(env.state);
        let t = env.shared.refresh_installed(env.state);
        env.spawn(t);
        Task::none()
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            Msg::Go(route) => env.go(route),
            Msg::Install { model_id, quant } => {
                let t = env.shared.install(env.state, &model_id, Some(quant));
                env.spawn(t);
            }
            Msg::Load(id) => {
                let t = env.shared.load_model(env.state, &id, true);
                env.spawn(t);
            }
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let profile = &cx.state.profile;
        let status = mesh_backend::connection_status(cx.state);

        let header = row![
            widgets::page_title("Dashboard"),
            space::horizontal(),
            mesh_pill(&status),
        ]
        .align_y(Alignment::Center);

        // ── System ──────────────────────────────────────────────────
        let mut gauges = column![widgets::gauge(
            "Memory Usage",
            profile
                .memory
                .total_bytes
                .saturating_sub(profile.memory.available_bytes),
            profile.memory.total_bytes,
        )]
        .spacing(12)
        .width(FillPortion(3));
        if let Some(gpu) = profile.gpus.first() {
            gauges = gauges.push(widgets::gauge("GPU VRAM", 0, gpu.vram_bytes));
        }
        let cpu = &profile.cpu;
        let gpu_row = match profile.gpus.first() {
            Some(gpu) => hardware_row(
                icon::MONITOR,
                "GPU",
                gpu.name.clone(),
                format!("{} · {}", format_bytes(gpu.vram_bytes), gpu.compute_api),
            ),
            None => hardware_row(
                icon::MONITOR,
                "GPU",
                "None detected".into(),
                "CPU-only inference".into(),
            ),
        };
        let hardware = container(widgets::card(
            Pad::Md,
            column![
                widgets::overline("Hardware"),
                hardware_row(
                    icon::CPU,
                    "CPU",
                    cpu.model_name.clone(),
                    format!("{}c / {}t", cpu.physical_cores, cpu.logical_cores)
                ),
                hardware_row(
                    icon::MEMORY,
                    "RAM",
                    format_bytes(profile.memory.total_bytes),
                    format!("{:.1} GB/s", profile.memory.estimated_bandwidth_gbps)
                ),
                gpu_row,
                hardware_row(
                    icon::HARD_DRIVE,
                    "Storage",
                    format!("{} free", format_bytes(profile.storage.available_bytes)),
                    format!("{:.0} MB/s read", profile.storage.estimated_read_speed_mbps)
                ),
            ]
            .spacing(10),
        ))
        .width(FillPortion(2));
        let system = row![gauges, hardware].spacing(16);

        // ── Discovery ───────────────────────────────────────────────
        let mut discovery =
            column![
                widgets::label("Recommended for This Device", BODY, TEXT_SECONDARY)
                    .font(widgets::BOLD)
            ]
            .spacing(8);
        match cx.shared.recommendations.as_deref() {
            None => discovery = discovery.push(widgets::muted("Calculating...")),
            Some([]) => {
                discovery =
                    discovery.push(widgets::muted("No models found for this hardware profile."))
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
                    let mut right = row![].spacing(12).align_y(Alignment::Center);
                    if let Some(size) = format_download_size(rec.estimated_download_bytes) {
                        right = right.push(widgets::label(size, SMALL, TEXT_SECONDARY));
                    }
                    right = right
                        .push(widgets::muted(format!(
                            "{} RAM",
                            format_bytes(rec.estimated_memory_usage_bytes)
                        )))
                        .push(widgets::label(format_tps(tps), SMALL, speed))
                        .push(widgets::mono(
                            format!("{:.0}%", rec.confidence * 100.0),
                            SMALL,
                            TEXT_MUTED,
                        ));
                    discovery = discovery.push(widgets::clickable_card(
                        Pad::Sm,
                        row![
                            widgets::strong(rec.model_name.clone()),
                            widgets::badge(rec.quantization.to_string(), Tone::Default),
                            space::horizontal(),
                            right,
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        Msg::Go(Route::Models),
                    ));
                }
            }
        }

        let mut page = column![header, self.hero(cx), system, discovery].spacing(16);
        if !cx.shared.installed.is_empty() {
            page = page.push(widgets::clickable_card(
                Pad::Sm,
                row![
                    icon(icon::USERS).color(PAW_500),
                    widgets::label(
                        "Share your idle compute with the hive. Help other bears run AI on devices that can't do it alone.",
                        SMALL,
                        TEXT_SECONDARY
                    ),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                Msg::Go(Route::Mesh),
            ));
        }
        scroll_page(page)
    }

    /// The top card: a running download, first-run install, the loaded
    /// model, or the installed models to load.
    fn hero<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let shared = cx.shared;
        if let Some(job) = &shared.install {
            return widgets::download_progress(&job.model_id, job.progress.as_ref());
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
            return widgets::accent_card(
                Pad::Lg,
                row![
                    icon(icon::LIGHTNING).size(24).color(PAW_500),
                    column![
                        widgets::label("Get Started", 18.0, TEXT_PRIMARY).font(widgets::BOLD),
                        widgets::secondary(if is_fallback {
                            "Start with a small, fast model that runs on almost any machine. You can add bigger ones from the Model Browser."
                        } else {
                            "HiveBear analyzed your hardware and picked a model that runs well here. One click to install and start chatting."
                        }),
                        row![
                            widgets::btn(
                                Kind::Primary,
                                Some(icon::DOWNLOAD_SIMPLE),
                                label,
                                Some(Msg::Install {
                                    model_id: top.model_id.clone(),
                                    quant: top.quantization.to_string(),
                                })
                            ),
                            widgets::muted(detail),
                        ]
                        .spacing(16)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(8),
                ]
                .spacing(16),
            )
            .into();
        }

        if let Some(first) = shared.loaded.first() {
            let mut detail = format!("Ready · ctx {}", thousands(first.context_length.into()));
            if let Some(layers) = first.gpu_layers {
                detail.push_str(&format!(" · {layers} GPU layers"));
            }
            let mut content = column![row![
                widgets::dot(SUCCESS, 5.0),
                column![
                    row![
                        widgets::strong(model_display_name(&first.model_path)),
                        widgets::badge(first.engine.to_string(), Tone::Default),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    widgets::muted(detail),
                ]
                .spacing(2),
                space::horizontal(),
                widgets::btn(
                    Kind::Primary,
                    Some(icon::CHAT_CIRCLE),
                    "Open Chat",
                    Some(Msg::Go(Route::Chat))
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center)]
            .spacing(10);
            if shared.loaded.len() > 1 {
                let mut others = row![].spacing(6);
                for m in shared.loaded.iter().skip(1) {
                    others = others.push(widgets::badge(
                        format!("● {}", model_display_name(&m.model_path)),
                        Tone::Accent,
                    ));
                }
                content = content.push(others);
            }
            return widgets::accent_card(Pad::Lg, content).into();
        }

        if has_models {
            let count = shared.installed.len();
            let mut buttons = row![].spacing(8);
            for m in shared.installed.iter().take(3) {
                let busy = shared.loading_model.as_deref() == Some(m.id.as_str());
                let enabled = shared.loading_model.is_none();
                buttons = buttons.push(widgets::btn_sm(
                    Kind::Secondary,
                    Some(if busy {
                        icon::ARROWS_CLOCKWISE
                    } else {
                        icon::LIGHTNING
                    }),
                    m.name.clone(),
                    enabled.then(|| Msg::Load(m.id.clone())),
                ));
            }
            return widgets::accent_card(
                Pad::Lg,
                column![
                    row![
                        icon(icon::CHAT_CIRCLE).size(20).color(PAW_500),
                        column![
                            widgets::strong("Ready to chat"),
                            widgets::label(
                                format!(
                                    "{count} model{} installed. Load one to start.",
                                    if count > 1 { "s" } else { "" }
                                ),
                                SMALL,
                                TEXT_SECONDARY
                            ),
                        ],
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                    buttons,
                ]
                .spacing(10),
            )
            .into();
        }
        space().into()
    }
}

fn hardware_row<'a>(
    glyph: &'a str,
    title: &'a str,
    value: String,
    detail: String,
) -> Element<'a, Msg> {
    row![
        icon(glyph).size(14).color(TEXT_MUTED),
        column![
            widgets::label(title.to_uppercase(), TINY, TEXT_MUTED),
            text(value).size(SMALL).font(widgets::BOLD),
            widgets::label(detail, TINY, TEXT_MUTED),
        ],
    ]
    .spacing(10)
    .into()
}

/// Compact mesh-status pill for the dashboard header.
///
/// This is the retention lever: users need to see that the mesh is running
/// and that they are connected to others.
fn mesh_pill<'a>(status: &MeshConnectionStatus) -> Element<'a, Msg> {
    // Running locally but unregistered is not "connecting" — nothing will
    // change until the coordination server comes back. Registered with
    // nobody else online is a settled state, not a pending one.
    let (label, glyph, color) = if !status.running {
        ("Mesh idle".to_string(), icon::GRAPH, TEXT_MUTED)
    } else if !status.registered {
        ("Mesh offline".to_string(), icon::WIFI_SLASH, WARNING)
    } else if status.peer_count == 0 {
        (
            "You're early · no peers yet".to_string(),
            icon::GRAPH,
            PAW_400,
        )
    } else {
        let n = status.peer_count;
        (
            format!("Mesh active · {n} peer{}", if n == 1 { "" } else { "s" }),
            icon::USERS,
            SUCCESS,
        )
    };
    let muted = color == TEXT_MUTED;
    button(
        row![icon(glyph).size(SMALL), text(label).size(SMALL)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .padding([5, 12])
    .style(move |_, _| button::Style {
        background: Some(
            if muted {
                SURFACE_OVERLAY
            } else {
                tint(color, 0.1)
            }
            .into(),
        ),
        text_color: color,
        border: iced::Border {
            color: if muted { BORDER } else { tint(color, 0.3) },
            width: 1.0,
            radius: 999.0.into(),
        },
        ..Default::default()
    })
    .on_press(Msg::Go(Route::Mesh))
    .into()
}
