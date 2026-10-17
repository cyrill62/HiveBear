//! Colours, spacing and fonts, carried over from the web app's design tokens
//! (`apps/desktop/src/styles/index.css`) so both builds look like one product.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle,
};

// ── Brand ("paw") ───────────────────────────────────────────────────
pub const PAW_400: Color32 = Color32::from_rgb(0xe8, 0x7a, 0x54);
pub const PAW_500: Color32 = Color32::from_rgb(0xe0, 0x5c, 0x31);
pub const PAW_600: Color32 = Color32::from_rgb(0xc9, 0x47, 0x23);

// ── Surfaces ────────────────────────────────────────────────────────
pub const SURFACE: Color32 = Color32::from_rgb(0x11, 0x11, 0x13);
pub const SURFACE_RAISED: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1f);
pub const SURFACE_OVERLAY: Color32 = Color32::from_rgb(0x25, 0x25, 0x28);
pub const BORDER: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x32);

// ── Text ────────────────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xee, 0xee, 0xef);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x9f, 0x9f, 0xa6);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x6b, 0x6b, 0x73);

// ── Status ──────────────────────────────────────────────────────────
pub const SUCCESS: Color32 = Color32::from_rgb(0x2d, 0xcc, 0x70);
pub const WARNING: Color32 = Color32::from_rgb(0xf7, 0xb7, 0x31);
pub const DANGER: Color32 = Color32::from_rgb(0xe7, 0x4c, 0x3c);

// ── Shape ───────────────────────────────────────────────────────────
pub const RADIUS_SM: u8 = 6;
pub const RADIUS_MD: u8 = 8;
pub const RADIUS_LG: u8 = 12;
pub const RADIUS_XL: u8 = 16;

/// A colour at a fraction of its opacity — the `bg-paw-500/10` idiom.
pub fn tint(color: Color32, alpha: f32) -> Color32 {
    color.gamma_multiply(alpha)
}

pub fn shadow_overlay() -> Shadow {
    Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(100),
    }
}

pub fn shadow_floating() -> Shadow {
    Shadow {
        offset: [0, 12],
        blur: 40,
        spread: 0,
        color: Color32::from_black_alpha(128),
    }
}

/// Text sizes: the web app is built on Tailwind's `text-sm` (14px) body,
/// `text-xs` (12px) secondary text and 11px labels.
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;
pub const TINY: f32 = 11.0;

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(20.0)),
            (TextStyle::Body, FontId::proportional(BODY)),
            (TextStyle::Button, FontId::proportional(BODY)),
            (TextStyle::Small, FontId::proportional(SMALL)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();

        let s = &mut style.spacing;
        s.item_spacing = egui::vec2(8.0, 8.0);
        s.button_padding = egui::vec2(12.0, 6.0);
        s.interact_size.y = 28.0;
        s.window_margin = Margin::same(16);
        s.menu_margin = Margin::same(6);
        s.slider_width = 160.0;

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = SURFACE;
        v.window_fill = SURFACE_RAISED;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(RADIUS_XL);
        v.window_shadow = shadow_overlay();
        v.popup_shadow = shadow_overlay();
        v.menu_corner_radius = CornerRadius::same(RADIUS_MD);
        v.extreme_bg_color = SURFACE;
        v.text_edit_bg_color = Some(SURFACE);
        v.faint_bg_color = SURFACE_RAISED;
        v.code_bg_color = SURFACE_OVERLAY;
        v.hyperlink_color = PAW_400;
        v.warn_fg_color = WARNING;
        v.error_fg_color = DANGER;
        v.selection.bg_fill = tint(PAW_500, 0.35);
        v.selection.stroke = Stroke::new(1.0, PAW_400);
        v.slider_trailing_fill = true;

        let radius = CornerRadius::same(RADIUS_MD);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE_RAISED;
        w.noninteractive.weak_bg_fill = SURFACE_RAISED;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        w.noninteractive.corner_radius = radius;

        w.inactive.bg_fill = SURFACE_OVERLAY;
        w.inactive.weak_bg_fill = SURFACE_RAISED;
        w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT_SECONDARY);
        w.inactive.corner_radius = radius;

        w.hovered.bg_fill = SURFACE_OVERLAY;
        w.hovered.weak_bg_fill = SURFACE_OVERLAY;
        w.hovered.bg_stroke = Stroke::new(1.0, tint(PAW_500, 0.5));
        w.hovered.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = SURFACE_OVERLAY;
        w.active.weak_bg_fill = SURFACE_OVERLAY;
        w.active.bg_stroke = Stroke::new(1.0, PAW_500);
        w.active.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;

        w.open.bg_fill = SURFACE_OVERLAY;
        w.open.weak_bg_fill = SURFACE_OVERLAY;
        w.open.bg_stroke = Stroke::new(1.0, PAW_500);
        w.open.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        w.open.corner_radius = radius;
    });
}
