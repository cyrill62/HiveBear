//! Colours, carried over from the web app's design tokens
//! (`apps/desktop/src/styles/index.css`) so every build looks like one
//! product, and applied to GPUI Kit's theme so its components match.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{px, rgb, App, Hsla};

// Brand ("paw")
pub const PAW_400: u32 = 0xe87a54;
pub const PAW_500: u32 = 0xe05c31;
pub const PAW_600: u32 = 0xc94723;

// Surfaces
pub const SURFACE: u32 = 0x111113;
pub const SURFACE_RAISED: u32 = 0x1c1c1f;
pub const SURFACE_OVERLAY: u32 = 0x252528;
pub const BORDER: u32 = 0x2e2e32;

// Text
pub const TEXT_PRIMARY: u32 = 0xeeeeef;
pub const TEXT_SECONDARY: u32 = 0x9f9fa6;
pub const TEXT_MUTED: u32 = 0x6b6b73;

// Status
pub const SUCCESS: u32 = 0x2dcc70;
pub const WARNING: u32 = 0xf7b731;
pub const DANGER: u32 = 0xe74c3c;

/// A design-token colour.
pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// A colour at a fraction of its opacity — the `bg-paw-500/10` idiom.
pub fn ca(hex: u32, alpha: f32) -> Hsla {
    c(hex).opacity(alpha)
}

/// Dark mode, with GPUI Kit's palette replaced by HiveBear's.
pub fn apply(cx: &mut App) {
    // Mode first: switching mode loads that mode's registered theme, which
    // would replace colour edits made in the same step.
    Theme::change(ThemeMode::Dark, None, cx);
    // Edits through `update` also replace the derived component tokens
    // (button backgrounds come from those) and refresh every window.
    Theme::update(cx, recolor);
}

fn recolor(t: &mut Theme) {
    t.radius = px(8.);
    t.radius_lg = px(12.);
    t.font_size = px(14.);

    t.background = c(SURFACE);
    t.foreground = c(TEXT_PRIMARY);
    t.border = c(BORDER);
    t.input = c(BORDER);
    t.ring = ca(PAW_500, 0.5);
    t.caret = c(PAW_400);
    t.selection = ca(PAW_500, 0.35);
    t.muted = c(SURFACE_OVERLAY);
    t.muted_foreground = c(TEXT_MUTED);
    t.accent = c(SURFACE_OVERLAY);
    t.accent_foreground = c(TEXT_PRIMARY);
    t.popover = c(SURFACE_RAISED);
    t.popover_foreground = c(TEXT_PRIMARY);
    t.link = c(PAW_400);
    t.link_hover = c(PAW_500);

    t.primary = c(PAW_500);
    t.primary_hover = c(PAW_600);
    t.primary_active = c(PAW_600);
    t.primary_foreground = c(0xffffff);
    t.button_primary = c(PAW_500);
    t.button_primary_hover = c(PAW_600);
    t.button_primary_active = c(PAW_600);
    t.button_primary_foreground = c(0xffffff);

    t.secondary = c(SURFACE_OVERLAY);
    t.secondary_hover = c(BORDER);
    t.secondary_active = c(BORDER);
    t.secondary_foreground = c(TEXT_PRIMARY);

    t.success = c(SUCCESS);
    t.warning = c(WARNING);
    t.danger = c(DANGER);
    t.progress_bar = c(PAW_500);
    t.slider_bar = c(PAW_500);
    t.slider_thumb = c(0xffffff);
    t.switch = c(SURFACE_OVERLAY);
    t.scrollbar_thumb = c(BORDER);
    t.scrollbar_thumb_hover = c(TEXT_MUTED);
    t.overlay = ca(0x000000, 0.6);
    t.list_hover = c(SURFACE_OVERLAY);
    t.list_active = ca(PAW_500, 0.12);
    t.list_active_border = ca(PAW_500, 0.4);
    t.tab_bar = c(SURFACE_OVERLAY);
    t.tab_active = c(SURFACE_RAISED);
    t.tab_active_foreground = c(TEXT_PRIMARY);
    t.tab_foreground = c(TEXT_MUTED);
}
