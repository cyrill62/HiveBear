//! Ctrl/⌘+K command palette.

use iced::widget::{button, column, container, operation, row, rule, text, text_input, Id};
use iced::{Alignment, Element, Fill, Task};

use super::icon::{self, icon};
use super::theme::{self, *};
use super::widgets::{self, Kind};
use super::Route;

const INPUT_ID: &str = "command-palette-input";

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

#[derive(Debug, Clone)]
pub enum Msg {
    Query(String),
    Up,
    Down,
    Submit,
    Pick(usize),
    Close,
}

/// Where the palette sends you, and whether it asked for a fresh chat.
pub struct Choice {
    pub route: Route,
    pub new_chat: bool,
}

#[derive(Default)]
pub struct Palette {
    open: bool,
    query: String,
    selected: usize,
}

impl Palette {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle<T>(&mut self) -> Task<T> {
        self.open = !self.open;
        if !self.open {
            return Task::none();
        }
        self.query.clear();
        self.selected = 0;
        operation::focus(Id::new(INPUT_ID))
    }

    fn filtered(&self) -> Vec<&'static Command> {
        let q = self.query.to_lowercase();
        COMMANDS
            .iter()
            .filter(|c| {
                q.is_empty() || c.label.to_lowercase().contains(&q) || c.keywords.contains(&q)
            })
            .collect()
    }

    pub fn update(&mut self, msg: Msg) -> Option<Choice> {
        let count = self.filtered().len();
        let pick = match msg {
            Msg::Query(q) => {
                self.query = q;
                self.selected = 0;
                None
            }
            Msg::Down => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
                None
            }
            Msg::Up => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            Msg::Submit => Some(self.selected),
            Msg::Pick(i) => Some(i),
            Msg::Close => {
                self.open = false;
                None
            }
        };
        let cmd = pick.and_then(|i| self.filtered().get(i).copied())?;
        self.open = false;
        Some(Choice {
            route: cmd.route,
            new_chat: cmd.new_chat,
        })
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let mut list = column![].spacing(0);
        let filtered = self.filtered();
        if filtered.is_empty() {
            list = list.push(
                container(widgets::muted("No results"))
                    .center_x(Fill)
                    .padding(16),
            );
        }
        for (i, cmd) in filtered.into_iter().enumerate() {
            list = list.push(
                button(
                    row![
                        icon(cmd.glyph).size(14).color(TEXT_MUTED),
                        text(cmd.label).size(BODY)
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([8, 16])
                .style(theme::button(Kind::Row {
                    selected: i == self.selected,
                }))
                .on_press(Msg::Pick(i)),
            );
        }
        container(column![
            container(
                row![
                    icon(icon::COMMAND).color(TEXT_MUTED),
                    text_input("Type a command...", &self.query)
                        .id(Id::new(INPUT_ID))
                        .on_input(Msg::Query)
                        .on_submit(Msg::Submit)
                        .size(BODY)
                        .padding(4)
                        .style(|t, s| iced::widget::text_input::Style {
                            border: iced::Border::default(),
                            background: iced::Color::TRANSPARENT.into(),
                            ..theme::input(t, s)
                        }),
                    widgets::mono("esc", 10.0, TEXT_MUTED),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            )
            .padding([10, 16]),
            rule::horizontal(1),
            container(list).padding([4, 0]),
        ])
        .width(440)
        .style(theme::floating)
        .into()
    }
}
