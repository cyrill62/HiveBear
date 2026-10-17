use eframe::egui::{self, Align, Layout, RichText, Ui};
use egui_phosphor::regular as icon;

use crate::backend::mesh::{self as mesh_backend, MeshConnectionStatus};
use crate::ui::theme::*;
use crate::ui::util::{
    format_bytes, format_download_size, format_tps, model_display_name, thousands,
};
use crate::ui::welcome::first_model;
use crate::ui::widgets::{self, Btn, Pad, Tone};
use crate::ui::{Env, Route};

#[derive(Default)]
pub struct Dashboard;

impl Dashboard {
    pub fn enter(&mut self, env: &mut Env) {
        env.shared.refresh_installed(env.ctx);
        env.shared.refresh_loaded(env.ctx);
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        let profile = env.state().profile.clone();

        ui.horizontal(|ui| {
            widgets::page_title(ui, "Dashboard");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if mesh_pill(ui, &mesh_backend::connection_status(env.state())).clicked() {
                    env.go(Route::Mesh);
                }
            });
        });

        self.hero(ui, env);

        // ── System ──────────────────────────────────────────────────
        widgets::split(
            ui,
            0.6,
            |left| {
                widgets::gauge(
                    left,
                    "Memory Usage",
                    profile
                        .memory
                        .total_bytes
                        .saturating_sub(profile.memory.available_bytes),
                    profile.memory.total_bytes,
                );
                if let Some(gpu) = profile.gpus.first() {
                    widgets::gauge(left, "GPU VRAM", 0, gpu.vram_bytes);
                }
            },
            |ui| {
                widgets::card(ui, Pad::Md, |ui| {
                    widgets::overline(ui, "Hardware");
                    ui.add_space(4.0);
                    let cpu = &profile.cpu;
                    hardware_row(
                        ui,
                        icon::CPU,
                        "CPU",
                        &cpu.model_name,
                        &format!("{}c / {}t", cpu.physical_cores, cpu.logical_cores),
                    );
                    hardware_row(
                        ui,
                        icon::MEMORY,
                        "RAM",
                        &format_bytes(profile.memory.total_bytes),
                        &format!("{:.1} GB/s", profile.memory.estimated_bandwidth_gbps),
                    );
                    match profile.gpus.first() {
                        Some(gpu) => hardware_row(
                            ui,
                            icon::MONITOR,
                            "GPU",
                            &gpu.name,
                            &format!("{} · {}", format_bytes(gpu.vram_bytes), gpu.compute_api),
                        ),
                        None => hardware_row(
                            ui,
                            icon::MONITOR,
                            "GPU",
                            "None detected",
                            "CPU-only inference",
                        ),
                    }
                    hardware_row(
                        ui,
                        icon::HARD_DRIVE,
                        "Storage",
                        &format!("{} free", format_bytes(profile.storage.available_bytes)),
                        &format!("{:.0} MB/s read", profile.storage.estimated_read_speed_mbps),
                    );
                });
            },
        );

        // ── Discovery ───────────────────────────────────────────────
        ui.label(
            RichText::new("Recommended for This Device")
                .strong()
                .color(TEXT_SECONDARY),
        );
        match env.shared.recommendations.as_deref() {
            None => {
                ui.label(widgets::muted("Calculating..."));
            }
            Some([]) => {
                ui.label(widgets::muted("No models found for this hardware profile."));
            }
            Some(recs) => {
                let mut open_models = false;
                for rec in recs.iter().take(3) {
                    let id = format!("rec-{}-{}", rec.model_id, rec.quantization);
                    let response = widgets::clickable_card(ui, id, Pad::Sm, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(widgets::text(&rec.model_name).strong());
                            widgets::badge(ui, &rec.quantization.to_string(), Tone::Default);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{:.0}%", rec.confidence * 100.0))
                                        .font(mono(SMALL))
                                        .color(TEXT_MUTED),
                                );
                                let tps = rec.estimated_tokens_per_sec;
                                let color = if tps >= 20.0 {
                                    SUCCESS
                                } else if tps >= 10.0 {
                                    WARNING
                                } else {
                                    DANGER
                                };
                                ui.label(RichText::new(format_tps(tps)).size(SMALL).color(color));
                                ui.label(widgets::muted(format!(
                                    "{} RAM",
                                    format_bytes(rec.estimated_memory_usage_bytes)
                                )))
                                .on_hover_text("Memory while running");
                                if let Some(size) =
                                    format_download_size(rec.estimated_download_bytes)
                                {
                                    ui.label(widgets::secondary(size).size(SMALL))
                                        .on_hover_text("Download size");
                                }
                            });
                        });
                    });
                    open_models |= response.clicked();
                }
                if open_models {
                    env.go(Route::Models);
                }
            }
        }

        if !env.shared.installed.is_empty() {
            let response = widgets::clickable_card(ui, "mesh-callout", Pad::Sm, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::USERS).color(PAW_500));
                    ui.label(widgets::secondary(
                        "Share your idle compute with the hive. Help other bears run AI on devices that can't do it alone.",
                    ).size(SMALL));
                });
            });
            if response.clicked() {
                env.go(Route::Mesh);
            }
        }
    }

    /// The top card: a running download, first-run install, the loaded
    /// model, or the installed models to load.
    fn hero(&mut self, ui: &mut Ui, env: &mut Env) {
        if let Some(job) = &env.shared.install {
            widgets::download_progress(ui, &job.model_id, job.progress().as_ref());
            return;
        }

        let has_models = !env.shared.installed.is_empty();
        let loading_lists = !env.shared.installed_loaded || env.shared.recommendations_loading();

        if !has_models && !loading_lists {
            // Never an empty first-run card: fall back to a tiny model if the
            // recommender found nothing.
            let (top, is_fallback) =
                first_model(env.shared.recommendations.as_deref().unwrap_or_default());
            let size = format_download_size(top.estimated_download_bytes);
            widgets::accent_card(ui, Pad::Lg, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::LIGHTNING).size(24.0).color(PAW_500));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Get Started").size(18.0).strong().color(TEXT_PRIMARY));
                        ui.add(egui::Label::new(widgets::secondary(if is_fallback {
                            "Start with a small, fast model that runs on almost any machine. You can add bigger ones from the Model Browser."
                        } else {
                            "HiveBear analyzed your hardware and picked a model that runs well here. One click to install and start chatting."
                        })).wrap());
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            let label = format!(
                                "Install {} ({}{})",
                                top.model_name,
                                top.quantization,
                                size.as_ref().map(|s| format!(", {s}")).unwrap_or_default()
                            );
                            if Btn::primary(icon::DOWNLOAD_SIMPLE, &label).show(ui).clicked() {
                                env.shared.install(env.ctx, &top.model_id, Some(top.quantization.to_string()));
                            }
                            let mut detail = size.map(|s| format!("{s} download · ")).unwrap_or_default();
                            detail.push_str(&format!("{} RAM", format_bytes(top.estimated_memory_usage_bytes)));
                            if top.estimated_tokens_per_sec > 0.0 {
                                detail.push_str(&format!(" · {}", format_tps(top.estimated_tokens_per_sec)));
                            }
                            ui.label(widgets::muted(detail));
                        });
                    });
                });
            });
            return;
        }

        if let Some(first) = env.shared.loaded.first().cloned() {
            let others: Vec<String> = env
                .shared
                .loaded
                .iter()
                .skip(1)
                .map(|m| model_display_name(&m.model_path))
                .collect();
            widgets::accent_card(ui, Pad::Lg, |ui| {
                ui.horizontal(|ui| {
                    widgets::dot(ui, SUCCESS, 5.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(widgets::text(model_display_name(&first.model_path)).strong());
                            widgets::badge(ui, &first.engine.to_string(), Tone::Default);
                        });
                        let mut detail =
                            format!("Ready · ctx {}", thousands(first.context_length.into()));
                        if let Some(layers) = first.gpu_layers {
                            detail.push_str(&format!(" · {layers} GPU layers"));
                        }
                        ui.label(widgets::muted(detail));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Btn::primary(icon::CHAT_CIRCLE, "Open Chat")
                            .show(ui)
                            .clicked()
                        {
                            env.go(Route::Chat);
                        }
                    });
                });
                if !others.is_empty() {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        for name in &others {
                            widgets::badge(ui, &format!("● {name}"), Tone::Accent);
                        }
                    });
                }
            });
            return;
        }

        if has_models {
            let count = env.shared.installed.len();
            let loading = env.shared.model_loading().is_some();
            let mut load = None;
            widgets::accent_card(ui, Pad::Lg, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::CHAT_CIRCLE).size(20.0).color(PAW_500));
                    ui.vertical(|ui| {
                        ui.label(widgets::text("Ready to chat").strong());
                        ui.label(
                            widgets::secondary(format!(
                                "{count} model{} installed. Load one to start.",
                                if count > 1 { "s" } else { "" }
                            ))
                            .size(SMALL),
                        );
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for m in env.shared.installed.iter().take(3) {
                        let busy = env.shared.model_loading() == Some(m.id.as_str());
                        if Btn::secondary(icon::LIGHTNING, &m.name)
                            .small()
                            .enabled(!loading)
                            .busy(busy)
                            .show(ui)
                            .clicked()
                        {
                            load = Some(m.id.clone());
                        }
                    }
                });
            });
            if let Some(id) = load {
                env.shared.load_model(env.ctx, &id, true);
            }
        }
    }
}

fn hardware_row(ui: &mut Ui, glyph: &str, label: &str, value: &str, detail: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(glyph).size(14.0).color(TEXT_MUTED));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(label.to_uppercase())
                    .size(TINY)
                    .color(TEXT_MUTED),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(value)
                        .size(SMALL)
                        .strong()
                        .color(TEXT_PRIMARY),
                )
                .truncate(),
            );
            ui.add(egui::Label::new(RichText::new(detail).size(TINY).color(TEXT_MUTED)).truncate());
        });
    });
}

pub const EARLY_MESSAGE: &str =
    "You're early — no peers online yet. Your machine will help others as the hive grows.";

/// Compact mesh-status pill for the dashboard header.
///
/// This is the retention lever: users need to see that the mesh is running
/// and that they are connected to others.
fn mesh_pill(ui: &mut Ui, status: &MeshConnectionStatus) -> egui::Response {
    // Running locally but unregistered is not "connecting" — nothing will
    // change until the coordination server comes back. Registered with
    // nobody else online is a settled state, not a pending one.
    let (label, glyph, color, tip) = if !status.running {
        ("Mesh idle".to_string(), icon::GRAPH, TEXT_MUTED, None)
    } else if !status.registered {
        ("Mesh offline".to_string(), icon::WIFI_SLASH, WARNING, None)
    } else if status.peer_count == 0 {
        (
            "You're early · no peers yet".to_string(),
            icon::GRAPH,
            PAW_400,
            Some(EARLY_MESSAGE),
        )
    } else {
        (
            format!(
                "Mesh active · {} peer{}",
                status.peer_count,
                if status.peer_count == 1 { "" } else { "s" }
            ),
            icon::USERS,
            SUCCESS,
            None,
        )
    };
    let fill = if color == TEXT_MUTED {
        SURFACE_OVERLAY
    } else {
        tint(color, 0.1)
    };
    let response = ui
        .add(
            egui::Button::new(
                RichText::new(format!("{glyph}  {label}"))
                    .size(SMALL)
                    .color(color),
            )
            .fill(fill)
            .stroke(egui::Stroke::new(
                1.0,
                if color == TEXT_MUTED {
                    BORDER
                } else {
                    tint(color, 0.3)
                },
            ))
            .corner_radius(egui::CornerRadius::same(255)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    match tip {
        Some(t) => response.on_hover_text(t),
        None => response,
    }
}
