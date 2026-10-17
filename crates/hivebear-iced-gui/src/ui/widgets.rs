//! The small component kit the pages are built from — the iced counterparts
//! of `components/ui/*` in the web app (Card, Button, Badge, …).

use iced::widget::{
    button, column, container, progress_bar, row, space, text, text_input, tooltip, Button,
    Container, Text,
};
use iced::{font, Alignment, Color, Element, Fill, Font, Length, Theme};

use super::icon::{self, icon};
pub use super::theme::Kind;
use super::theme::{self, *};
use super::util::format_bytes;
use hivebear_registry::download::DownloadProgress;

pub const MONO: Font = Font::MONOSPACE;
pub const BOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

// ── Text ────────────────────────────────────────────────────────────

pub fn label<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color) -> Text<'a> {
    text(s).size(size).color(color)
}

pub fn primary<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    label(s, BODY, TEXT_PRIMARY)
}

pub fn strong<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    primary(s).font(BOLD)
}

pub fn secondary<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    label(s, BODY, TEXT_SECONDARY)
}

pub fn muted<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    label(s, SMALL, TEXT_MUTED)
}

pub fn mono<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color) -> Text<'a> {
    label(s, size, color).font(MONO)
}

pub fn page_title<'a>(s: &'a str) -> Text<'a> {
    label(s, 20.0, TEXT_PRIMARY).font(BOLD)
}

pub fn section_title<'a>(s: &'a str) -> Text<'a> {
    strong(s)
}

/// The small uppercase label above groups ("HARDWARE", "CONVERSATIONS").
pub fn overline<'a>(s: &str) -> Text<'a> {
    label(s.to_uppercase(), TINY, TEXT_MUTED).font(BOLD)
}

// ── Containers ──────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub enum Pad {
    Sm,
    Md,
    Lg,
}

impl Pad {
    fn px(self) -> u16 {
        match self {
            Pad::Sm => 12,
            Pad::Md => 16,
            Pad::Lg => 20,
        }
    }
}

pub fn card<'a, M: 'a>(pad: Pad, content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    container(content)
        .padding(pad.px())
        .width(Fill)
        .style(theme::card)
}

pub fn accent_card<'a, M: 'a>(pad: Pad, content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    container(content)
        .padding(pad.px())
        .width(Fill)
        .style(theme::accent_card)
}

/// A card that highlights on hover and sends `on_press` when clicked.
pub fn clickable_card<'a, M: Clone + 'a>(
    pad: Pad,
    content: impl Into<Element<'a, M>>,
    on_press: M,
) -> Button<'a, M> {
    button(content)
        .padding(pad.px())
        .width(Fill)
        .style(theme::button(Kind::Card))
        .on_press(on_press)
}

pub fn banner<'a, M: 'a>(color: Color, message: impl text::IntoFragment<'a>) -> Element<'a, M> {
    container(text(message).size(BODY))
        .padding([8, 14])
        .width(Fill)
        .style(theme::banner(color))
        .into()
}

pub fn error_banner<'a, M: 'a>(message: impl text::IntoFragment<'a>) -> Element<'a, M> {
    banner(DANGER, message)
}

/// A glyph centred on a rounded square.
pub fn icon_tile<'a, M: 'a>(
    glyph: &'a str,
    size: f32,
    fill: Color,
    color: Color,
) -> Element<'a, M> {
    container(icon(glyph).size(size / 2.0).color(color))
        .width(size)
        .height(size)
        .center(size)
        .style(theme::pill(fill, RADIUS_LG))
        .into()
}

/// Centered icon, title, description and optional actions.
pub fn empty_state<'a, M: 'a>(
    glyph: &'a str,
    title: &'a str,
    description: &'a str,
    actions: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mut col = column![
        icon_tile(glyph, 48.0, SURFACE_OVERLAY, TEXT_MUTED),
        strong(title),
        muted(description),
    ]
    .spacing(6)
    .align_x(Alignment::Center);
    if let Some(actions) = actions {
        col = col.push(actions);
    }
    container(col).padding([24, 0]).center_x(Fill).into()
}

// ── Buttons ─────────────────────────────────────────────────────────

/// A button with an optional leading icon. `on_press = None` disables it.
pub fn btn<'a, M: Clone + 'a>(
    kind: Kind,
    glyph: Option<&'a str>,
    text_label: impl text::IntoFragment<'a>,
    on_press: Option<M>,
) -> Button<'a, M> {
    sized_btn(kind, glyph, text_label, on_press, BODY, [7, 14])
}

pub fn btn_sm<'a, M: Clone + 'a>(
    kind: Kind,
    glyph: Option<&'a str>,
    text_label: impl text::IntoFragment<'a>,
    on_press: Option<M>,
) -> Button<'a, M> {
    sized_btn(kind, glyph, text_label, on_press, SMALL, [4, 10])
}

fn sized_btn<'a, M: Clone + 'a>(
    kind: Kind,
    glyph: Option<&'a str>,
    text_label: impl text::IntoFragment<'a>,
    on_press: Option<M>,
    size: f32,
    padding: [u16; 2],
) -> Button<'a, M> {
    let mut content = row![].spacing(8).align_y(Alignment::Center);
    if let Some(g) = glyph {
        content = content.push(icon(g).size(size + 1.0));
    }
    content = content.push(text(text_label).size(size));
    button(content)
        .padding(padding)
        .style(theme::button(kind))
        .on_press_maybe(on_press)
}

/// A button that shows progress instead of its label while `busy`.
pub fn busy_btn<'a, M: Clone + 'a>(
    kind: Kind,
    glyph: &'a str,
    text_label: &'a str,
    busy_label: &'a str,
    busy: bool,
    on_press: Option<M>,
) -> Button<'a, M> {
    if busy {
        btn(kind, Some(icon::ARROWS_CLOCKWISE), busy_label, None)
    } else {
        btn(kind, Some(glyph), text_label, on_press)
    }
}

/// A borderless icon button with a tooltip.
pub fn icon_button<'a, M: Clone + 'a>(glyph: &'a str, tip: &'a str, on_press: M) -> Element<'a, M> {
    tooltip(
        button(icon(glyph).size(15).color(TEXT_MUTED))
            .padding(6)
            .style(theme::button(Kind::Ghost))
            .on_press(on_press),
        container(text(tip).size(SMALL))
            .padding([4, 8])
            .style(theme::toast),
        tooltip::Position::Bottom,
    )
    .into()
}

pub fn link<'a, M: Clone + 'a>(
    text_label: impl text::IntoFragment<'a>,
    color: Color,
    on_press: M,
) -> Element<'a, M> {
    button(text(text_label).size(BODY).color(color))
        .padding(0)
        .style(|_: &Theme, _| button::Style::default())
        .on_press(on_press)
        .into()
}

pub fn chip<'a, M: Clone + 'a>(text_label: &'a str, on_press: M) -> Element<'a, M> {
    button(text(text_label).size(SMALL))
        .padding([4, 10])
        .style(theme::button(Kind::Chip))
        .on_press(on_press)
        .into()
}

/// Segmented tab control.
pub fn segmented<'a, M: Clone + 'a>(
    labels: Vec<String>,
    selected: usize,
    on_select: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let mut tabs = row![].spacing(2);
    for (i, l) in labels.into_iter().enumerate() {
        let on = i == selected;
        tabs = tabs.push(
            button(container(text(l).size(BODY)).center_x(Fill))
                .width(Fill)
                .padding(5)
                .style(theme::button(Kind::Row { selected: on }))
                .on_press(on_select(i)),
        );
    }
    container(tabs)
        .padding(2)
        .width(Fill)
        .style(theme::pill(SURFACE_OVERLAY, RADIUS_MD))
        .into()
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
    pub fn color(self) -> Color {
        match self {
            Tone::Default => TEXT_SECONDARY,
            Tone::Accent => PAW_400,
            Tone::Success => SUCCESS,
            Tone::Warning => WARNING,
            Tone::Danger => DANGER,
        }
    }
}

pub fn badge<'a, M: 'a>(text_label: impl text::IntoFragment<'a>, tone: Tone) -> Element<'a, M> {
    let color = tone.color();
    let fill = match tone {
        Tone::Default => SURFACE_OVERLAY,
        _ => tint(color, 0.12),
    };
    container(label(text_label, TINY, color))
        .padding([1, 6])
        .style(theme::pill(fill, RADIUS_SM))
        .into()
}

pub fn dot<'a, M: 'a>(color: Color, radius: f32) -> Element<'a, M> {
    container(space())
        .width(radius * 2.0)
        .height(radius * 2.0)
        .style(theme::pill(color, radius))
        .into()
}

/// A thin rounded progress bar.
pub fn bar<'a, M: 'a>(fraction: f32, color: Color, height: f32) -> Element<'a, M> {
    progress_bar(0.0..=1.0, fraction.clamp(0.0, 1.0))
        .girth(height)
        .style(move |_| progress_bar::Style {
            background: SURFACE_OVERLAY.into(),
            bar: color.into(),
            border: iced::Border {
                radius: (height / 2.0).into(),
                ..Default::default()
            },
        })
        .into()
}

/// "Memory Usage   12.0 GB / 32.0 GB" with a colour-coded bar.
pub fn gauge<'a, M: 'a>(title: &'a str, used: u64, total: u64) -> Element<'a, M> {
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
    card(
        Pad::Md,
        column![
            row![
                secondary(title),
                space::horizontal(),
                mono(
                    format!("{} / {}", gauge_value(used), gauge_value(total)),
                    SMALL,
                    TEXT_MUTED
                ),
            ]
            .align_y(Alignment::Center),
            bar(pct, color, 8.0),
        ]
        .spacing(10),
    )
    .into()
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
pub fn stat<'a, M: 'a>(title: &'a str, value: String, highlight: bool) -> Element<'a, M> {
    let value = mono(value, 13.0, if highlight { PAW_400 } else { TEXT_PRIMARY });
    column![
        muted(title),
        if highlight {
            value.font(Font {
                weight: font::Weight::Bold,
                ..MONO
            })
        } else {
            value
        }
    ]
    .spacing(2)
    .width(Fill)
    .into()
}

pub fn download_progress<'a, M: 'a>(
    model_id: &'a str,
    progress: Option<&DownloadProgress>,
) -> Element<'a, M> {
    let body: Element<'a, M> = match progress {
        None => row![
            icon(icon::DOWNLOAD_SIMPLE).size(SMALL).color(PAW_400),
            secondary("Starting download for"),
            strong(model_id),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        Some(p) => {
            let total = p
                .total_bytes
                .map(|t| format!(" / {}", format_bytes(t)))
                .unwrap_or_default();
            let fraction = p
                .total_bytes
                .filter(|t| *t > 0)
                .map_or(0.0, |t| p.bytes_downloaded as f32 / t as f32);
            column![
                row![
                    label(model_id, SMALL, TEXT_PRIMARY).font(BOLD),
                    space::horizontal(),
                    mono(
                        format!(
                            "{}{total} · {}/s",
                            format_bytes(p.bytes_downloaded),
                            format_bytes(p.bytes_per_sec as u64)
                        ),
                        SMALL,
                        TEXT_MUTED
                    ),
                ],
                bar(fraction, PAW_500, 6.0),
            ]
            .spacing(8)
            .into()
        }
    };
    container(body)
        .padding(12)
        .width(Fill)
        .style(|_: &Theme| iced::widget::container::Style {
            background: Some(SURFACE_RAISED.into()),
            border: iced::Border {
                color: tint(PAW_500, 0.3),
                width: 1.0,
                radius: RADIUS_LG.into(),
            },
            ..Default::default()
        })
        .into()
}

// ── Inputs ──────────────────────────────────────────────────────────

pub fn input<'a, M: Clone + 'a>(
    placeholder: &str,
    value: &str,
    on_input: impl Fn(String) -> M + 'a,
) -> iced::widget::TextInput<'a, M> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding([7, 10])
        .size(BODY)
        .style(theme::input)
}

/// A settings row: label and hint on the left, the control on the right.
pub fn field<'a, M: 'a>(
    title: &'a str,
    hint: Option<&'a str>,
    control: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let mut left = column![primary(title)]
        .spacing(2)
        .width(Length::FillPortion(1));
    if let Some(h) = hint {
        left = left.push(muted(h));
    }
    row![left, container(control).align_right(Length::FillPortion(1)),]
        .spacing(16)
        .align_y(Alignment::Center)
        .into()
}
