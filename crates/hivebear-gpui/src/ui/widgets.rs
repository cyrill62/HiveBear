//! The small component kit the pages are built from: GPUI Kit components
//! styled to match the web app (`components/ui/*`), plus a few HiveBear
//! specifics (gauges, stats, download progress).

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::theme::*;
use super::util::format_bytes;
use hivebear_registry::download::DownloadProgress;

// ── Text ────────────────────────────────────────────────────────────

pub fn page_title(text: impl Into<SharedString>) -> Div {
    div()
        .text_xl()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c(TEXT_PRIMARY))
        .child(text.into())
}

pub fn section_title(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c(TEXT_PRIMARY))
        .child(text.into())
}

/// The small uppercase label above groups ("HARDWARE", "CONVERSATIONS").
pub fn overline(text: &str) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c(TEXT_MUTED))
        .child(text.to_uppercase())
}

pub fn muted(text: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(c(TEXT_MUTED)).child(text.into())
}

pub fn secondary(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .text_color(c(TEXT_SECONDARY))
        .child(text.into())
}

pub fn mono(text: impl Into<SharedString>, color: u32) -> Div {
    div()
        .text_xs()
        .font_family("monospace")
        .text_color(c(color))
        .child(text.into())
}

// ── Containers ──────────────────────────────────────────────────────

pub fn card() -> Div {
    div()
        .w_full()
        .p_4()
        .rounded(px(12.))
        .bg(c(SURFACE_RAISED))
        .border_1()
        .border_color(c(BORDER))
}

/// Accent border and a faint tint, for hero states.
pub fn accent_card() -> Div {
    card()
        .p_5()
        .bg(ca(PAW_500, 0.06))
        .border_color(ca(PAW_500, 0.3))
}

/// A floating card (update notice).
pub fn raised_card() -> Div {
    div()
        .px_4()
        .py_3()
        .rounded(px(12.))
        .bg(c(SURFACE_RAISED))
        .border_1()
        .border_color(c(BORDER))
        .shadow_lg()
}

/// A card that highlights on hover and is clickable.
pub fn clickable_card(id: impl Into<ElementId>) -> Stateful<Div> {
    card()
        .p_3()
        .id(id)
        .cursor_pointer()
        .hover(|s| s.bg(c(SURFACE_OVERLAY)).border_color(ca(PAW_500, 0.4)))
}

pub fn inset() -> Div {
    div()
        .w_full()
        .px_3()
        .py_2()
        .rounded(px(8.))
        .bg(c(SURFACE))
        .border_1()
        .border_color(c(BORDER))
}

/// A coloured message box (errors, confirmations).
pub fn banner(color: u32, message: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .px_4()
        .py_2()
        .rounded(px(8.))
        .bg(ca(color, 0.1))
        .border_1()
        .border_color(ca(color, 0.3))
        .text_sm()
        .text_color(c(color))
        .child(message.into())
}

/// An icon centred on a rounded square.
pub fn icon_tile(icon: IconName, size: f32, fill: Hsla, color: Hsla) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .bg(fill)
        .child(Icon::new(icon).size(px(size / 2.)).text_color(color))
}

/// Centered icon, title and description; callers add actions as children.
pub fn empty_state(
    icon: IconName,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
) -> Div {
    v_flex()
        .w_full()
        .py_6()
        .gap_1p5()
        .items_center()
        .child(icon_tile(icon, 48., c(SURFACE_OVERLAY), c(TEXT_MUTED)))
        .child(
            div()
                .pt_1()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(title.into()),
        )
        .child(muted(description))
}

// ── Buttons ─────────────────────────────────────────────────────────

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label)
}

/// A borderless icon button with a tooltip.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id).icon(icon).ghost().small().tooltip(tooltip)
}

/// A suggestion chip ("llama", "mistral", …).
pub fn chip(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label).outline().xsmall()
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
    pub fn color(self) -> u32 {
        match self {
            Tone::Default => TEXT_SECONDARY,
            Tone::Accent => PAW_400,
            Tone::Success => SUCCESS,
            Tone::Warning => WARNING,
            Tone::Danger => DANGER,
        }
    }
}

pub fn badge(label: impl Into<SharedString>, tone: Tone) -> Div {
    let color = tone.color();
    div()
        .flex_shrink_0()
        .px_1p5()
        .rounded(px(6.))
        .text_size(px(11.))
        .text_color(c(color))
        .bg(match tone {
            Tone::Default => c(SURFACE_OVERLAY),
            _ => ca(color, 0.12),
        })
        .child(label.into())
}

pub fn dot(color: Hsla, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .bg(color)
}

/// A thin rounded progress bar.
pub fn bar(fraction: f32, color: Hsla, height: f32) -> Div {
    let fraction = fraction.clamp(0., 1.);
    div()
        .w_full()
        .h(px(height))
        .rounded_full()
        .bg(c(SURFACE_OVERLAY))
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(color)
                .w(relative(fraction)),
        )
}

/// "Memory Usage   12.0 GB / 32.0 GB" with a colour-coded bar.
pub fn gauge(title: &'static str, used: u64, total: u64) -> Div {
    let pct = if total > 0 {
        used as f32 / total as f32
    } else {
        0.
    };
    let color = if pct > 0.9 {
        DANGER
    } else if pct > 0.7 {
        WARNING
    } else {
        SUCCESS
    };
    card().child(
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .child(secondary(title))
                    .child(mono(
                        format!("{} / {}", gauge_value(used), gauge_value(total)),
                        TEXT_MUTED,
                    )),
            )
            .child(bar(pct, c(color), 8.)),
    )
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
pub fn stat(title: &'static str, value: impl Into<SharedString>, highlight: bool) -> Div {
    v_flex().flex_1().gap_0p5().child(muted(title)).child(
        div()
            .text_sm()
            .font_family("monospace")
            .text_color(c(if highlight { PAW_400 } else { TEXT_PRIMARY }))
            .when(highlight, |d| d.font_weight(FontWeight::SEMIBOLD))
            .child(value.into()),
    )
}

pub fn download_progress(model_id: &str, progress: Option<&DownloadProgress>) -> Div {
    let body = match progress {
        None => h_flex()
            .gap_2()
            .text_sm()
            .child(
                Icon::new(IconName::Download)
                    .size(px(14.))
                    .text_color(c(PAW_400)),
            )
            .child(
                div()
                    .text_color(c(TEXT_SECONDARY))
                    .child("Starting download for"),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(model_id.to_string()),
            )
            .into_any_element(),
        Some(p) => {
            let total = p
                .total_bytes
                .map(|t| format!(" / {}", format_bytes(t)))
                .unwrap_or_default();
            let fraction = p
                .total_bytes
                .filter(|t| *t > 0)
                .map_or(0., |t| p.bytes_downloaded as f32 / t as f32);
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .child(model_id.to_string()),
                        )
                        .child(mono(
                            format!(
                                "{}{total} · {}/s",
                                format_bytes(p.bytes_downloaded),
                                format_bytes(p.bytes_per_sec as u64)
                            ),
                            TEXT_MUTED,
                        )),
                )
                .child(bar(fraction, c(PAW_500), 6.))
                .into_any_element()
        }
    };
    card().p_3().border_color(ca(PAW_500, 0.3)).child(body)
}

/// A settings row: label and hint on the left, the control on the right.
pub fn field(title: &'static str, hint: Option<&'static str>, control: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .gap_4()
        .justify_between()
        .child(
            v_flex()
                .max_w(px(320.))
                .gap_0p5()
                .child(div().text_sm().child(title))
                .when_some(hint, |d, h| d.child(muted(h))),
        )
        .child(control)
}

/// A padded, vertically scrolling page body (the web app's `Surface`).
pub fn scroll_page(id: &'static str, content: impl IntoElement) -> Stateful<Div> {
    div()
        .id(id)
        .size_full()
        .overflow_y_scroll()
        .child(div().p_6().child(content))
}
