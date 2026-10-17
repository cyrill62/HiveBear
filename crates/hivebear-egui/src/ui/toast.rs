//! Transient notifications in the bottom-right corner.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, CornerRadius, Frame, Margin, RichText, Stroke};
use egui_phosphor::regular as icon;

use super::theme::*;

const LIFETIME: Duration = Duration::from_secs(4);

#[derive(Clone, Copy)]
pub enum Level {
    Success,
    Error,
}

struct Toast {
    id: u64,
    message: String,
    level: Level,
    shown_at: Instant,
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    pub fn push(&mut self, message: impl Into<String>, level: Level) {
        let message = message.into();
        if let Level::Error = level {
            tracing::warn!("{message}");
        }
        self.items.push(Toast {
            id: self.next_id,
            message,
            level,
            shown_at: Instant::now(),
        });
        self.next_id += 1;
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.push(message, Level::Success);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(message, Level::Error);
    }

    /// Draw the stack. `bottom_offset` leaves room for the update card.
    pub fn show(&mut self, ctx: &egui::Context, bottom_offset: f32) {
        self.items.retain(|t| t.shown_at.elapsed() < LIFETIME);
        if self.items.is_empty() {
            return;
        }
        if let Some(oldest) = self.items.iter().map(|t| t.shown_at).min() {
            ctx.request_repaint_after(LIFETIME.saturating_sub(oldest.elapsed()));
        }

        let mut dismissed = None;
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, [-16.0, -16.0 - bottom_offset])
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                for toast in &self.items {
                    let (glyph, color) = match toast.level {
                        Level::Success => (icon::CHECK_CIRCLE, SUCCESS),
                        Level::Error => (icon::WARNING_CIRCLE, DANGER),
                    };
                    Frame::new()
                        .fill(SURFACE_RAISED)
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(CornerRadius::same(RADIUS_LG))
                        .inner_margin(Margin::symmetric(14, 10))
                        .shadow(shadow_overlay())
                        .show(ui, |ui| {
                            ui.set_min_width(280.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(glyph).size(16.0).color(color));
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&toast.message).color(TEXT_PRIMARY),
                                    )
                                    .wrap(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::TOP),
                                    |ui| {
                                        if super::widgets::icon_button(ui, icon::X, "Dismiss")
                                            .clicked()
                                        {
                                            dismissed = Some(toast.id);
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        if let Some(id) = dismissed {
            self.items.retain(|t| t.id != id);
        }
    }
}
