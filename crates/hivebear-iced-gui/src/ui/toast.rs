//! Transient notifications in the bottom-right corner.

use std::time::{Duration, Instant};

use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Fill};

use super::icon::{self, icon};
use super::theme::{self, *};
use super::widgets;
use super::Message;

const LIFETIME: Duration = Duration::from_secs(4);

#[derive(Clone, Copy)]
enum Level {
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
    fn push(&mut self, message: String, level: Level) {
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
        self.push(message.into(), Level::Success);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.push(message.into(), Level::Error);
    }

    pub fn dismiss(&mut self, id: u64) {
        self.items.retain(|t| t.id != id);
    }

    /// Drop expired toasts. Called on the timer tick.
    pub fn expire(&mut self) {
        self.items.retain(|t| t.shown_at.elapsed() < LIFETIME);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut stack = column![].spacing(6).width(360);
        for t in &self.items {
            let (glyph, color) = match t.level {
                Level::Success => (icon::CHECK_CIRCLE, SUCCESS),
                Level::Error => (icon::WARNING_CIRCLE, DANGER),
            };
            stack = stack.push(
                container(
                    row![
                        icon(glyph).size(16).color(color),
                        text(&t.message).size(BODY).width(Fill),
                        widgets::icon_button(icon::X, "Dismiss", Message::DismissToast(t.id)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([10, 14])
                .style(theme::toast),
            );
        }
        stack.into()
    }
}
