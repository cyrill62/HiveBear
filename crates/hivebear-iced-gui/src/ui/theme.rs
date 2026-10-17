//! Colours and widget styles, carried over from the web app's design tokens
//! (`apps/desktop/src/styles/index.css`) so every build looks like one product.

use iced::widget::{self, container, text_editor, text_input};
use iced::{Background, Border, Color, Shadow, Theme, Vector};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

// ── Brand ("paw") ───────────────────────────────────────────────────
pub const PAW_400: Color = rgb(0xe8, 0x7a, 0x54);
pub const PAW_500: Color = rgb(0xe0, 0x5c, 0x31);
pub const PAW_600: Color = rgb(0xc9, 0x47, 0x23);

// ── Surfaces ────────────────────────────────────────────────────────
pub const SURFACE: Color = rgb(0x11, 0x11, 0x13);
pub const SURFACE_RAISED: Color = rgb(0x1c, 0x1c, 0x1f);
pub const SURFACE_OVERLAY: Color = rgb(0x25, 0x25, 0x28);
pub const BORDER: Color = rgb(0x2e, 0x2e, 0x32);

// ── Text ────────────────────────────────────────────────────────────
pub const TEXT_PRIMARY: Color = rgb(0xee, 0xee, 0xef);
pub const TEXT_SECONDARY: Color = rgb(0x9f, 0x9f, 0xa6);
pub const TEXT_MUTED: Color = rgb(0x6b, 0x6b, 0x73);

// ── Status ──────────────────────────────────────────────────────────
pub const SUCCESS: Color = rgb(0x2d, 0xcc, 0x70);
pub const WARNING: Color = rgb(0xf7, 0xb7, 0x31);
pub const DANGER: Color = rgb(0xe7, 0x4c, 0x3c);

// ── Shape and type ──────────────────────────────────────────────────
pub const RADIUS_SM: f32 = 6.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;
pub const RADIUS_XL: f32 = 16.0;

/// Tailwind's `text-sm` body, `text-xs` secondary text and 11px labels.
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;
pub const TINY: f32 = 11.0;

/// A colour at a fraction of its opacity — the `bg-paw-500/10` idiom.
pub fn tint(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha,
        ..color
    }
}

pub fn app_theme() -> Theme {
    Theme::custom(
        "HiveBear",
        iced::theme::Palette {
            background: SURFACE,
            text: TEXT_PRIMARY,
            primary: PAW_500,
            success: SUCCESS,
            warning: WARNING,
            danger: DANGER,
        },
    )
}

fn border(color: Color, radius: f32) -> Border {
    Border {
        color,
        width: 1.0,
        radius: radius.into(),
    }
}

pub fn shadow_overlay() -> Shadow {
    Shadow {
        color: Color::from_rgba8(0, 0, 0, 0.4),
        offset: Vector::new(0.0, 4.0),
        blur_radius: 12.0,
    }
}

pub fn shadow_floating() -> Shadow {
    Shadow {
        color: Color::from_rgba8(0, 0, 0, 0.5),
        offset: Vector::new(0.0, 12.0),
        blur_radius: 40.0,
    }
}

// ── Containers ──────────────────────────────────────────────────────

pub fn page(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE_RAISED.into()),
        border: border(BORDER, RADIUS_LG),
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

/// Accent border and a faint tint, for hero states.
pub fn accent_card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(tint(PAW_500, 0.06).into()),
        border: border(tint(PAW_500, 0.3), RADIUS_LG),
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: border(BORDER, RADIUS_MD),
        ..Default::default()
    }
}

pub fn overlay_row(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE_OVERLAY.into()),
        border: Border {
            radius: RADIUS_MD.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A raised floating panel (modals, toasts, the save bar).
pub fn floating(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE_RAISED.into()),
        border: border(BORDER, RADIUS_XL),
        shadow: shadow_floating(),
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn toast(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE_RAISED.into()),
        border: border(BORDER, RADIUS_LG),
        shadow: shadow_overlay(),
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn backdrop(alpha: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, alpha).into()),
        ..Default::default()
    }
}

/// A coloured message box (errors, confirmations).
pub fn banner(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(tint(color, 0.1).into()),
        border: border(tint(color, 0.3), RADIUS_MD),
        text_color: Some(color),
        ..Default::default()
    }
}

/// A small rounded fill (badges, pills, icon tiles).
pub fn pill(fill: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(fill.into()),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ── Buttons ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
    /// A navigation or list row: transparent until hovered or selected.
    Row {
        selected: bool,
    },
    /// A card the whole of which is clickable.
    Card,
    Chip,
}

pub fn button(kind: Kind) -> impl Fn(&Theme, widget::button::Status) -> widget::button::Style {
    move |_, status| {
        use widget::button::{Status::*, Style};
        let hovered = matches!(status, Hovered | Pressed);
        let mut style = match kind {
            Kind::Primary => Style {
                background: Some(if hovered { PAW_600 } else { PAW_500 }.into()),
                text_color: Color::WHITE,
                border: Border {
                    radius: RADIUS_MD.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            Kind::Secondary => Style {
                background: Some(SURFACE_OVERLAY.into()),
                text_color: TEXT_PRIMARY,
                border: border(if hovered { tint(PAW_500, 0.5) } else { BORDER }, RADIUS_MD),
                ..Default::default()
            },
            Kind::Ghost => Style {
                background: hovered.then(|| Background::from(SURFACE_OVERLAY)),
                text_color: if hovered {
                    TEXT_PRIMARY
                } else {
                    TEXT_SECONDARY
                },
                border: Border {
                    radius: RADIUS_MD.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            Kind::Danger => Style {
                background: Some(tint(DANGER, if hovered { 0.25 } else { 0.15 }).into()),
                text_color: DANGER,
                border: border(tint(DANGER, 0.4), RADIUS_MD),
                ..Default::default()
            },
            Kind::Row { selected } => Style {
                background: if selected {
                    Some(SURFACE_OVERLAY.into())
                } else if hovered {
                    Some(tint(SURFACE_OVERLAY, 0.5).into())
                } else {
                    None
                },
                text_color: if selected || hovered {
                    TEXT_PRIMARY
                } else {
                    TEXT_SECONDARY
                },
                border: Border {
                    radius: RADIUS_MD.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            Kind::Card => Style {
                background: Some(
                    if hovered {
                        SURFACE_OVERLAY
                    } else {
                        SURFACE_RAISED
                    }
                    .into(),
                ),
                text_color: TEXT_PRIMARY,
                border: border(if hovered { tint(PAW_500, 0.4) } else { BORDER }, RADIUS_LG),
                ..Default::default()
            },
            Kind::Chip => Style {
                background: None,
                text_color: if hovered { PAW_400 } else { TEXT_SECONDARY },
                border: border(if hovered { tint(PAW_500, 0.3) } else { BORDER }, RADIUS_MD),
                ..Default::default()
            },
        };
        if status == Disabled {
            style.background = style.background.map(|b| b.scale_alpha(0.5));
            style.text_color = tint(style.text_color, 0.5);
        }
        style
    }
}

// ── Inputs ──────────────────────────────────────────────────────────

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: SURFACE.into(),
        border: border(if focused { PAW_500 } else { BORDER }, RADIUS_MD),
        icon: TEXT_MUTED,
        placeholder: TEXT_MUTED,
        value: TEXT_PRIMARY,
        selection: tint(PAW_500, 0.35),
    }
}

pub fn editor(_: &Theme, status: text_editor::Status) -> text_editor::Style {
    let focused = matches!(status, text_editor::Status::Focused { .. });
    text_editor::Style {
        background: SURFACE_RAISED.into(),
        border: border(if focused { tint(PAW_500, 0.5) } else { BORDER }, RADIUS_LG),
        placeholder: TEXT_MUTED,
        value: TEXT_PRIMARY,
        selection: tint(PAW_500, 0.35),
    }
}
