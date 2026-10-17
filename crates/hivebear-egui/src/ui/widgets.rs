//! The small component kit the pages are built from — the egui counterparts
//! of `components/ui/*` in the web app (Card, Button, Badge, Toggle, …).

use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, Response, RichText, Sense, Stroke,
    Ui, Vec2,
};
use egui_phosphor::regular as icon;

use super::theme::{self, *};
use super::util::format_bytes;
use hivebear_registry::download::DownloadProgress;

// ── Text ────────────────────────────────────────────────────────────

pub fn text(s: impl Into<String>) -> RichText {
    RichText::new(s).color(TEXT_PRIMARY)
}

pub fn secondary(s: impl Into<String>) -> RichText {
    RichText::new(s).color(TEXT_SECONDARY)
}

pub fn muted(s: impl Into<String>) -> RichText {
    RichText::new(s).color(TEXT_MUTED).size(SMALL)
}

pub fn page_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(20.0).strong().color(TEXT_PRIMARY));
}

pub fn section_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(BODY).strong().color(TEXT_PRIMARY));
}

/// The small uppercase label used above groups ("HARDWARE", "CONVERSATIONS").
pub fn overline(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title.to_uppercase())
            .size(TINY)
            .strong()
            .color(TEXT_MUTED),
    );
}

pub fn with_icon(glyph: &str, label: &str) -> String {
    if label.is_empty() {
        glyph.to_string()
    } else {
        format!("{glyph}  {label}")
    }
}

// ── Containers ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Pad {
    Sm,
    Md,
    Lg,
}

impl Pad {
    fn margin(self) -> Margin {
        match self {
            Pad::Sm => Margin::same(12),
            Pad::Md => Margin::same(16),
            Pad::Lg => Margin::same(20),
        }
    }
}

pub fn card_frame(pad: Pad) -> Frame {
    Frame::new()
        .fill(SURFACE_RAISED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(pad.margin())
}

/// A raised card that fills the available width.
pub fn card<R>(ui: &mut Ui, pad: Pad, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(pad)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card with an accent-coloured border and a faint tint, for hero states.
pub fn accent_card<R>(ui: &mut Ui, pad: Pad, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_frame(pad)
        .fill(tint(PAW_500, 0.06))
        .stroke(Stroke::new(1.0, tint(PAW_500, 0.3)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card that highlights on hover and reports clicks.
pub fn clickable_card(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    pad: Pad,
    add: impl FnOnce(&mut Ui),
) -> Response {
    let mut prepared = card_frame(pad).begin(ui);
    prepared
        .content_ui
        .set_width(prepared.content_ui.available_width());
    add(&mut prepared.content_ui);
    let rect = prepared.allocate_space(ui).rect;
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    if response.hovered() {
        prepared.frame.stroke = Stroke::new(1.0, tint(PAW_500, 0.4));
        prepared.frame.fill = SURFACE_OVERLAY;
    }
    prepared.paint(ui);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Two top-aligned columns side by side; `ratio` is the left one's share.
pub fn split(ui: &mut Ui, ratio: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    const GAP: f32 = 16.0;
    let total = ui.available_width();
    let left_w = (total - GAP) * ratio;
    let right_w = total - GAP - left_w;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        for (width, add) in [
            (left_w, Box::new(left) as Box<dyn FnOnce(&mut Ui)>),
            (right_w, Box::new(right)),
        ] {
            ui.allocate_ui_with_layout(Vec2::new(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 12.0;
                add(ui);
            });
        }
    });
}

/// A coloured message box (errors, confirmations).
pub fn banner(ui: &mut Ui, color: Color32, message: &str) {
    Frame::new()
        .fill(tint(color, 0.1))
        .stroke(Stroke::new(1.0, tint(color, 0.3)))
        .corner_radius(CornerRadius::same(RADIUS_MD))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(message).color(color));
        });
}

pub fn error_banner(ui: &mut Ui, message: &str) {
    banner(ui, DANGER, message);
}

/// Centered icon, title, description and optional actions.
pub fn empty_state(
    ui: &mut Ui,
    glyph: &str,
    title: &str,
    description: &str,
    actions: impl FnOnce(&mut Ui),
) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        icon_tile(ui, glyph, 48.0, SURFACE_OVERLAY, TEXT_MUTED);
        ui.add_space(8.0);
        ui.label(RichText::new(title).strong().color(TEXT_PRIMARY));
        ui.label(muted(description));
        ui.add_space(4.0);
        actions(ui);
        ui.add_space(24.0);
    });
}

/// A glyph centred on a rounded square. Painted rather than framed, so it
/// keeps its size inside centred layouts, where a `Frame` would stretch.
pub fn icon_tile(ui: &mut Ui, glyph: &str, size: f32, fill: Color32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(RADIUS_LG), fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(size / 2.0),
        color,
    );
}

// ── Buttons ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Size {
    Sm,
    Md,
}

pub struct Btn<'a> {
    label: String,
    kind: Kind,
    size: Size,
    enabled: bool,
    busy: bool,
    tooltip: Option<&'a str>,
}

impl<'a> Btn<'a> {
    pub fn new(glyph: &str, label: &str) -> Self {
        Self {
            label: with_icon(glyph, label),
            kind: Kind::Primary,
            size: Size::Md,
            enabled: true,
            busy: false,
            tooltip: None,
        }
    }

    pub fn primary(glyph: &str, label: &str) -> Self {
        Self::new(glyph, label)
    }

    pub fn secondary(glyph: &str, label: &str) -> Self {
        Self::new(glyph, label).kind(Kind::Secondary)
    }

    pub fn ghost(glyph: &str, label: &str) -> Self {
        Self::new(glyph, label).kind(Kind::Ghost)
    }

    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }

    pub fn small(mut self) -> Self {
        self.size = Size::Sm;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Show a spinner in place of the icon and disable the button.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    pub fn tooltip(mut self, tooltip: &'a str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (fill, fg, stroke) = match self.kind {
            Kind::Primary => (PAW_500, Color32::WHITE, Stroke::NONE),
            Kind::Secondary => (SURFACE_OVERLAY, TEXT_PRIMARY, Stroke::new(1.0, BORDER)),
            Kind::Ghost => (Color32::TRANSPARENT, TEXT_SECONDARY, Stroke::NONE),
            Kind::Danger => (
                tint(DANGER, 0.15),
                DANGER,
                Stroke::new(1.0, tint(DANGER, 0.4)),
            ),
        };
        let (font, pad, height) = match self.size {
            Size::Sm => (SMALL, egui::vec2(10.0, 4.0), 26.0),
            Size::Md => (BODY, egui::vec2(14.0, 7.0), 34.0),
        };

        let label = if self.busy {
            // The spinner is painted over the leading glyph's slot below.
            format!(
                "     {}",
                self.label
                    .split_once("  ")
                    .map_or(self.label.as_str(), |(_, l)| l)
            )
        } else {
            self.label
        };

        let button = egui::Button::new(RichText::new(label).size(font).color(fg))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(RADIUS_MD))
            .min_size(Vec2::new(0.0, height));

        let response = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding = pad;
                if self.kind == Kind::Primary {
                    let w = &mut ui.visuals_mut().widgets;
                    w.hovered.weak_bg_fill = PAW_600;
                    w.hovered.bg_fill = PAW_600;
                    w.active.weak_bg_fill = PAW_600;
                    w.active.bg_fill = PAW_600;
                    w.hovered.bg_stroke = Stroke::NONE;
                }
                ui.add_enabled(self.enabled && !self.busy, button)
            })
            .inner;

        if self.busy {
            let size = font;
            let center = egui::pos2(
                response.rect.left() + pad.x + size * 0.6,
                response.rect.center().y,
            );
            egui::Spinner::new()
                .size(size)
                .color(fg)
                .paint_at(ui, egui::Rect::from_center_size(center, Vec2::splat(size)));
        }

        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        match self.tooltip {
            Some(t) => response.on_hover_text(t),
            None => response,
        }
    }
}

/// A borderless icon button (toolbar actions, close buttons).
pub fn icon_button(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(glyph).size(15.0).color(TEXT_MUTED))
            .frame_when_inactive(false)
            .corner_radius(CornerRadius::same(RADIUS_MD))
            .min_size(Vec2::splat(28.0)),
    )
    .on_hover_text(tooltip)
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A text-only link-style button.
pub fn link_button(ui: &mut Ui, label: &str, color: Color32) -> Response {
    ui.add(egui::Button::new(RichText::new(label).color(color)).frame(false))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A suggestion chip ("llama", "mistral", …).
pub fn chip(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(SMALL).color(TEXT_SECONDARY))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(RADIUS_MD)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Segmented tab control. Returns the newly chosen index, if any.
pub fn segmented(ui: &mut Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let mut chosen = None;
    Frame::new()
        .fill(SURFACE_OVERLAY)
        .corner_radius(CornerRadius::same(RADIUS_MD))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let n = labels.len() as f32;
            let w = (ui.available_width() - 2.0 * (n - 1.0)) / n;
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (i, label) in labels.iter().enumerate() {
                    let on = i == selected;
                    let b = egui::Button::new(RichText::new(*label).color(if on {
                        TEXT_PRIMARY
                    } else {
                        TEXT_MUTED
                    }))
                    .fill(if on {
                        SURFACE_RAISED
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(RADIUS_SM))
                    .min_size(Vec2::new(w, 28.0));
                    if ui.add(b).clicked() && !on {
                        chosen = Some(i);
                    }
                }
            });
        });
    chosen
}

// ── Indicators ──────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum Tone {
    Default,
    Accent,
    Success,
    Warning,
    Danger,
}

impl Tone {
    pub fn color(self) -> Color32 {
        match self {
            Tone::Default => TEXT_SECONDARY,
            Tone::Accent => PAW_400,
            Tone::Success => SUCCESS,
            Tone::Warning => WARNING,
            Tone::Danger => DANGER,
        }
    }
}

pub fn badge(ui: &mut Ui, label: &str, tone: Tone) -> Response {
    let color = tone.color();
    let fill = match tone {
        Tone::Default => SURFACE_OVERLAY,
        _ => tint(color, 0.12),
    };
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(RADIUS_SM))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(TINY).color(color));
        })
        .response
}

/// A filled status dot.
pub fn dot(ui: &mut Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 2.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

pub fn spinner(ui: &mut Ui) {
    ui.add(egui::Spinner::new().size(14.0).color(PAW_500));
}

/// A thin rounded progress bar. `fraction = None` draws an indeterminate bar.
pub fn bar(ui: &mut Ui, fraction: Option<f32>, color: Color32, height: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = CornerRadius::same((height / 2.0) as u8);
    ui.painter().rect_filled(rect, radius, SURFACE_OVERLAY);
    let fill = match fraction {
        Some(f) => {
            let mut r = rect;
            r.set_width(rect.width() * f.clamp(0.0, 1.0));
            r
        }
        None => {
            // A sliding 30% segment, so an unknown total still shows motion.
            let t = ui.input(|i| i.time) as f32;
            let seg = rect.width() * 0.3;
            let x = rect.left() + (t * 0.6).fract() * (rect.width() + seg) - seg;
            ui.ctx().request_repaint();
            egui::Rect::from_min_max(
                egui::pos2(x.max(rect.left()), rect.top()),
                egui::pos2((x + seg).min(rect.right()), rect.bottom()),
            )
        }
    };
    if fill.width() > 0.0 {
        ui.painter().rect_filled(fill, radius, color);
    }
}

/// "Memory Usage 12.0 GB / 32.0 GB" with a colour-coded bar.
pub fn gauge(ui: &mut Ui, label: &str, used: u64, total: u64) {
    card(ui, Pad::Md, |ui| {
        let pct = if total > 0 {
            used as f32 / total as f32
        } else {
            0.0
        };
        let color = if pct > 0.9 {
            DANGER
        } else if pct > 0.7 {
            WARNING
        } else {
            SUCCESS
        };
        ui.horizontal(|ui| {
            ui.label(secondary(label));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} / {}", gauge_value(used), gauge_value(total)))
                        .font(theme::mono(SMALL))
                        .color(TEXT_MUTED),
                );
            });
        });
        ui.add_space(4.0);
        bar(ui, Some(pct), color, 8.0);
    });
}

fn gauge_value(n: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let n = n as f64;
    if n >= GB {
        format!("{:.1} GB", n / GB)
    } else if n >= MB {
        format!("{:.0} MB", n / MB)
    } else {
        format!("{n}")
    }
}

/// A labelled value in a results grid.
pub fn stat(ui: &mut Ui, label: &str, value: &str, highlight: bool) {
    ui.vertical(|ui| {
        ui.label(muted(label));
        let color = if highlight { PAW_400 } else { TEXT_PRIMARY };
        let mut rt = RichText::new(value).font(theme::mono(13.0)).color(color);
        if highlight {
            rt = rt.strong();
        }
        ui.label(rt);
    });
}

pub fn download_progress(ui: &mut Ui, model_id: &str, progress: Option<&DownloadProgress>) {
    card_frame(Pad::Sm)
        .stroke(Stroke::new(1.0, tint(PAW_500, 0.3)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(p) = progress else {
                ui.horizontal(|ui| {
                    spinner(ui);
                    ui.label(secondary("Starting download for"));
                    ui.label(text(model_id).strong());
                });
                return;
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(model_id)
                        .size(SMALL)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let total = p
                        .total_bytes
                        .map(|t| format!(" / {}", format_bytes(t)))
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!(
                            "{}{total} · {}/s",
                            format_bytes(p.bytes_downloaded),
                            format_bytes(p.bytes_per_sec as u64)
                        ))
                        .font(theme::mono(SMALL))
                        .color(TEXT_MUTED),
                    );
                });
            });
            let fraction = p
                .total_bytes
                .filter(|t| *t > 0)
                .map(|t| p.bytes_downloaded as f32 / t as f32);
            bar(ui, fraction, PAW_500, 6.0);
        });
}

// ── Inputs ──────────────────────────────────────────────────────────

/// An iOS-style switch, like the web app's `Toggle`.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = Vec2::new(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let bg = if *on { PAW_500 } else { SURFACE_OVERLAY };
    let radius = rect.height() / 2.0;
    ui.painter().rect(
        rect,
        CornerRadius::same(radius as u8),
        bg,
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), radius - 3.0, Color32::WHITE);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A single-line text field with the app's look.
pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(RichText::new(hint).color(TEXT_MUTED))
            .desired_width(width)
            .margin(Margin::symmetric(10, 6)),
    )
}

pub fn password_field(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    visible: bool,
    width: f32,
) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(RichText::new(hint).color(TEXT_MUTED))
            .password(!visible)
            .desired_width(width)
            .margin(Margin::symmetric(10, 6)),
    )
}

/// Search box with a leading magnifier.
pub fn search_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(
                RichText::new(format!("{}  {hint}", icon::MAGNIFYING_GLASS)).color(TEXT_MUTED),
            )
            .desired_width(width)
            .margin(Margin::symmetric(10, 6)),
    )
}
