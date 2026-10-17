//! "Update available" card, bottom-right.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, CornerRadius, Frame, Margin, RichText, Stroke};
use egui_phosphor::regular as icon;

use super::theme::*;
use super::widgets::{self, Btn};
use crate::backend::update;
use crate::tasks::{poll_slot, Task};

const RECHECK: Duration = Duration::from_secs(4 * 60 * 60);

pub struct UpdateNotice {
    task: Option<Task<Result<Option<String>, String>>>,
    available: Option<String>,
    dismissed: bool,
    next_check: Instant,
}

impl UpdateNotice {
    pub fn new(rt: &tokio::runtime::Handle, ctx: &egui::Context) -> Self {
        Self {
            task: Some(spawn_check(rt, ctx)),
            available: None,
            dismissed: false,
            next_check: Instant::now() + RECHECK,
        }
    }

    pub fn poll(&mut self, rt: &tokio::runtime::Handle, ctx: &egui::Context) {
        match poll_slot(&mut self.task) {
            Some(Ok(version)) => self.available = version,
            // Non-fatal for the user, but not something to swallow: a broken
            // update check is invisible by design, and v0.1.5 sat
            // un-updatable for six months because nothing reported this.
            Some(Err(e)) => crate::telemetry::report_error(&e, "updater"),
            None => {}
        }
        if self.task.is_none() && Instant::now() >= self.next_check {
            self.next_check = Instant::now() + RECHECK;
            self.task = Some(spawn_check(rt, ctx));
        }
        ctx.request_repaint_after(self.next_check.saturating_duration_since(Instant::now()));
    }

    /// Draw the card if there is an update. Returns the height it takes, so
    /// toasts can stack above it.
    pub fn show(&mut self, ctx: &egui::Context) -> f32 {
        let Some(version) = self.available.clone() else {
            return 0.0;
        };
        if self.dismissed {
            return 0.0;
        }

        let response = egui::Area::new(egui::Id::new("update-notice"))
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE_RAISED)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(RADIUS_LG))
                    .inner_margin(Margin::symmetric(14, 10))
                    .shadow(shadow_overlay())
                    .show(ui, |ui| {
                        ui.set_min_width(300.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(icon::DOWNLOAD_SIMPLE)
                                    .size(16.0)
                                    .color(PAW_400),
                            );
                            ui.vertical(|ui| {
                                ui.label(widgets::text("Update available").strong());
                                ui.label(widgets::muted(format!("v{version}")));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::icon_button(ui, icon::X, "Dismiss").clicked() {
                                        self.dismissed = true;
                                    }
                                    if Btn::primary("", "Download").small().show(ui).clicked() {
                                        super::util::open_url(ctx, update::RELEASES_URL);
                                    }
                                },
                            );
                        });
                    });
            })
            .response;
        response.rect.height() + 8.0
    }
}

fn spawn_check(
    rt: &tokio::runtime::Handle,
    ctx: &egui::Context,
) -> Task<Result<Option<String>, String>> {
    Task::spawn(rt, ctx, async {
        let client = reqwest::Client::new();
        update::check(&client).await
    })
}
