use std::time::{Duration, Instant};

use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, RichText, Ui};
use egui_phosphor::regular as icon;

use super::dashboard::EARLY_MESSAGE;
use crate::backend::mesh as mesh_backend;
use crate::error::CmdResult;
use crate::tasks::{poll_slot, Task};
use crate::ui::theme::*;
use crate::ui::widgets::{self, Btn, Kind, Pad, Tone};
use crate::ui::{Env, Route};

/// How long to watch a fresh join for a failed bind or registration, which
/// only shows up a moment after the node starts in the background.
const JOIN_WATCH: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Mesh {
    toggle_task: Option<Task<CmdResult<()>>>,
    join_task: Option<Task<CmdResult<mesh_backend::MeshConnectionStatus>>>,
    leave_task: Option<Task<CmdResult<()>>>,
    watching_join: Option<Instant>,
}

impl Mesh {
    fn poll(&mut self, env: &mut Env) {
        if let Some(Err(e)) = poll_slot(&mut self.toggle_task) {
            env.toasts
                .error(format!("Couldn't change the mesh setting: {e}"));
        }
        match poll_slot(&mut self.join_task) {
            Some(Ok(_)) => self.watching_join = Some(Instant::now()),
            Some(Err(e)) => env.toasts.error(format!("Couldn't join the hive: {e}")),
            None => {}
        }
        if let Some(Err(e)) = poll_slot(&mut self.leave_task) {
            env.toasts.error(format!("Couldn't leave the hive: {e}"));
        }
        if let Some(started) = self.watching_join {
            let status = mesh_backend::connection_status(env.state());
            if let Some(e) = status.last_error {
                env.toasts.error(format!("Couldn't join the hive: {e}"));
                self.watching_join = None;
            } else if status.registered || started.elapsed() > JOIN_WATCH {
                self.watching_join = None;
            } else {
                env.ctx
                    .egui
                    .request_repaint_after(Duration::from_millis(500));
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        self.poll(env);
        // Both reads are in-memory, so the page is always live; repaint
        // periodically to pick up peers arriving and heartbeats failing.
        ui.ctx().request_repaint_after(Duration::from_secs(5));
        let config = env.state().config().mesh;
        let connection = mesh_backend::connection_status(env.state());

        ui.horizontal(|ui| {
            widgets::page_title(ui, "P2P Mesh");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if Btn::secondary(icon::GEAR, "Configure")
                    .small()
                    .show(ui)
                    .clicked()
                {
                    env.go(Route::Settings);
                }
            });
        });

        // Status header
        widgets::card(ui, Pad::Lg, |ui| {
            ui.horizontal(|ui| {
                let color = if config.enabled { SUCCESS } else { TEXT_MUTED };
                status_icon(ui, icon::GRAPH, color);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let title = if config.enabled {
                            "Mesh Enabled"
                        } else {
                            "Mesh Disabled"
                        };
                        ui.label(widgets::text(title).size(16.0).strong());
                        let paid = config.tier == "paid";
                        widgets::badge(
                            ui,
                            if paid { "Paid" } else { "Free" },
                            if paid { Tone::Accent } else { Tone::Default },
                        );
                    });
                    ui.label(widgets::muted(if config.enabled {
                        format!("Listening on port {}", config.port)
                    } else {
                        "Enable to join the mesh network and share compute.".into()
                    }));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let kind = if config.enabled {
                        Kind::Secondary
                    } else {
                        Kind::Primary
                    };
                    let label = if config.enabled { "Disable" } else { "Enable" };
                    if Btn::new(icon::POWER, label)
                        .kind(kind)
                        .busy(self.toggle_task.is_some())
                        .show(ui)
                        .clicked()
                    {
                        let enable = !config.enabled;
                        self.toggle_task = Some(env.ctx.run(move |s| async move {
                            mesh_backend::set_mesh_enabled(&s, enable).await
                        }));
                    }
                });
            });
        });

        // Config cards
        ui.columns(3, |cols| {
            let server = config
                .coordination_server
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .to_string();
            stat_card(&mut cols[0], icon::GRAPH, "Coordination Server", &server);
            stat_card(
                &mut cols[1],
                icon::SHIELD,
                "Min Trust Score",
                &format!("{:.0}%", config.min_reputation * 100.0),
            );
            stat_card(
                &mut cols[2],
                icon::PULSE,
                "Verification Rate",
                &format!("{:.0}%", config.verification_rate * 100.0),
            );
        });

        // Contribution
        widgets::card(ui, Pad::Md, |ui| {
            widgets::section_title(ui, "Resource Contribution");
            ui.horizontal(|ui| {
                ui.label(widgets::muted("Share limit"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.0}%", config.max_contribution_percent * 100.0))
                            .font(mono(SMALL))
                            .color(TEXT_SECONDARY),
                    );
                });
            });
            widgets::bar(
                ui,
                Some(config.max_contribution_percent as f32),
                PAW_500,
                8.0,
            );
            ui.label(widgets::muted(
                "Maximum percentage of local resources shared with the mesh network.",
            ));
        });

        // Connection status
        let joining =
            self.join_task.is_some() || self.leave_task.is_some() || self.watching_join.is_some();
        widgets::card(ui, Pad::Lg, |ui| {
            ui.horizontal(|ui| {
                let (glyph, color) = if connection.registered {
                    (icon::WIFI_HIGH, SUCCESS)
                } else if connection.running {
                    (icon::WIFI_SLASH, WARNING)
                } else {
                    (icon::WIFI_SLASH, TEXT_MUTED)
                };
                status_icon(ui, glyph, color);
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - 110.0);
                    // "Connected" means the coordination server has
                    // acknowledged us, not merely that a local socket is open.
                    ui.horizontal(|ui| {
                        let title = if connection.registered {
                            "Connected to Hive"
                        } else if connection.running {
                            "Running locally — not on the hive"
                        } else {
                            "Not Connected"
                        };
                        ui.label(widgets::text(title).size(16.0).strong());
                        if connection.registered {
                            let n = connection.peer_count;
                            widgets::badge(ui, &format!("{n} peer{}", if n == 1 { "" } else { "s" }), Tone::Default);
                        }
                    });
                    let detail = if connection.registered && connection.peer_count == 0 {
                        EARLY_MESSAGE.to_string()
                    } else if connection.registered {
                        let id: String = connection.node_id.as_deref().unwrap_or_default().chars().take(12).collect();
                        format!("Node {id}… registered and sending heartbeats")
                    } else if connection.running {
                        "The coordination server is unreachable, so other peers cannot find you. Retrying in the background.".into()
                    } else if let Some(e) = &connection.last_error {
                        format!("Last attempt failed: {e}")
                    } else {
                        "Join to share idle compute with other bears. Nothing is shared until you do.".into()
                    };
                    ui.add(egui::Label::new(widgets::muted(detail)).wrap());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (kind, glyph, label) = if connection.running {
                        (Kind::Secondary, icon::WIFI_SLASH, "Leave")
                    } else {
                        (Kind::Primary, icon::WIFI_HIGH, "Join")
                    };
                    if Btn::new(glyph, label).kind(kind).busy(joining).show(ui).clicked() {
                        if connection.running {
                            self.leave_task = Some(env.ctx.run(|s| async move { mesh_backend::leave_mesh(&s).await }));
                        } else {
                            self.join_task = Some(env.ctx.blocking(mesh_backend::join_mesh));
                        }
                    }
                });
            });
        });
    }
}

fn status_icon(ui: &mut Ui, glyph: &str, color: egui::Color32) {
    Frame::new()
        .fill(if color == TEXT_MUTED {
            SURFACE_OVERLAY
        } else {
            tint(color, 0.1)
        })
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.label(RichText::new(glyph).size(24.0).color(color));
        });
}

fn stat_card(ui: &mut Ui, glyph: &str, label: &str, value: &str) {
    widgets::card(ui, Pad::Md, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(glyph).size(14.0).color(TEXT_MUTED));
            ui.label(
                RichText::new(label.to_uppercase())
                    .size(TINY)
                    .color(TEXT_MUTED),
            );
        });
        ui.add(egui::Label::new(widgets::text(value).strong()).truncate());
    });
}
