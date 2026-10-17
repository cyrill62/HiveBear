//! Ctrl/⌘+K command palette.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Key, Margin, RichText, Sense, Stroke, Vec2,
};
use egui_phosphor::regular as icon;

use super::theme::*;
use super::Route;

struct Command {
    label: &'static str,
    glyph: &'static str,
    route: Route,
    keywords: &'static str,
    new_chat: bool,
}

const COMMANDS: &[Command] = &[
    Command {
        label: "New Chat",
        glyph: icon::PLUS,
        route: Route::Chat,
        keywords: "conversation",
        new_chat: true,
    },
    Command {
        label: "Go to Dashboard",
        glyph: icon::SQUARES_FOUR,
        route: Route::Dashboard,
        keywords: "",
        new_chat: false,
    },
    Command {
        label: "Browse Models",
        glyph: icon::MAGNIFYING_GLASS,
        route: Route::Models,
        keywords: "search install download",
        new_chat: false,
    },
    Command {
        label: "Open Chat",
        glyph: icon::CHAT_CIRCLE,
        route: Route::Chat,
        keywords: "",
        new_chat: false,
    },
    Command {
        label: "Run Benchmark",
        glyph: icon::GAUGE,
        route: Route::Benchmark,
        keywords: "performance speed",
        new_chat: false,
    },
    Command {
        label: "Mesh Status",
        glyph: icon::GRAPH,
        route: Route::Mesh,
        keywords: "p2p peer network",
        new_chat: false,
    },
    Command {
        label: "Account",
        glyph: icon::USER_CIRCLE,
        route: Route::Account,
        keywords: "login auth subscription",
        new_chat: false,
    },
    Command {
        label: "Settings",
        glyph: icon::GEAR,
        route: Route::Settings,
        keywords: "preferences config",
        new_chat: false,
    },
];

#[derive(Default)]
pub struct Palette {
    open: bool,
    query: String,
    selected: usize,
    focus_pending: bool,
}

impl Palette {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.query.clear();
            self.selected = 0;
            self.focus_pending = true;
        }
    }

    /// The chosen destination, and whether it asked for a fresh chat.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<(Route, bool)> {
        if !self.open {
            return None;
        }

        let q = self.query.to_lowercase();
        let filtered: Vec<&Command> = COMMANDS
            .iter()
            .filter(|c| {
                q.is_empty() || c.label.to_lowercase().contains(&q) || c.keywords.contains(&q)
            })
            .collect();
        self.selected = self.selected.min(filtered.len().saturating_sub(1));

        let (down, up, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
            )
        });
        if down {
            self.selected = (self.selected + 1).min(filtered.len().saturating_sub(1));
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }

        let mut chosen = enter
            .then(|| filtered.get(self.selected).map(|c| (c.route, c.new_chat)))
            .flatten();

        let modal = egui::Modal::new(egui::Id::new("command-palette"))
            .backdrop_color(Color32::from_black_alpha(100))
            .area(
                egui::Modal::default_area(egui::Id::new("command-palette-area")).anchor(
                    egui::Align2::CENTER_TOP,
                    [0.0, ctx.content_rect().height() * 0.2],
                ),
            )
            .frame(
                Frame::new()
                    .fill(SURFACE_RAISED)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(RADIUS_XL))
                    .shadow(shadow_floating()),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                Frame::new()
                    .inner_margin(Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon::COMMAND).color(TEXT_MUTED));
                            let edit = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .hint_text(RichText::new("Type a command...").color(TEXT_MUTED))
                                    .frame(Frame::NONE)
                                    .desired_width(ui.available_width() - 40.0),
                            );
                            if self.focus_pending {
                                edit.request_focus();
                                self.focus_pending = false;
                            }
                            if edit.changed() {
                                self.selected = 0;
                            }
                            ui.label(RichText::new("esc").font(mono(10.0)).color(TEXT_MUTED));
                        });
                    });
                ui.separator();
                Frame::new()
                    .inner_margin(Margin::symmetric(0, 4))
                    .show(ui, |ui| {
                        if filtered.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(16.0);
                                ui.label(RichText::new("No results").color(TEXT_MUTED));
                                ui.add_space(16.0);
                            });
                        }
                        for (i, cmd) in filtered.iter().enumerate() {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 34.0),
                                Sense::click(),
                            );
                            if response.hovered() {
                                self.selected = i;
                            }
                            let active = i == self.selected;
                            if active {
                                ui.painter()
                                    .rect_filled(rect, CornerRadius::ZERO, SURFACE_OVERLAY);
                            }
                            let y = rect.center().y;
                            ui.painter().text(
                                egui::pos2(rect.left() + 24.0, y),
                                egui::Align2::CENTER_CENTER,
                                cmd.glyph,
                                egui::FontId::proportional(14.0),
                                TEXT_MUTED,
                            );
                            ui.painter().text(
                                egui::pos2(rect.left() + 44.0, y),
                                egui::Align2::LEFT_CENTER,
                                cmd.label,
                                egui::FontId::proportional(BODY),
                                if active { TEXT_PRIMARY } else { TEXT_SECONDARY },
                            );
                            if response.clicked() {
                                chosen = Some((cmd.route, cmd.new_chat));
                            }
                        }
                    });
            });

        if modal.should_close() || chosen.is_some() {
            self.open = false;
        }
        chosen
    }
}
