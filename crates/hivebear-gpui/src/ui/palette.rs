//! Ctrl/⌘+K command palette, shown in a GPUI Kit dialog.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::theme::*;
use super::Route;

struct Command {
    label: &'static str,
    icon: IconName,
    route: Route,
    keywords: &'static str,
    new_chat: bool,
}

const COMMANDS: &[Command] = &[
    Command {
        label: "New Chat",
        icon: IconName::Plus,
        route: Route::Chat,
        keywords: "conversation",
        new_chat: true,
    },
    Command {
        label: "Go to Dashboard",
        icon: IconName::LayoutDashboard,
        route: Route::Dashboard,
        keywords: "",
        new_chat: false,
    },
    Command {
        label: "Browse Models",
        icon: IconName::Search,
        route: Route::Models,
        keywords: "search install download",
        new_chat: false,
    },
    Command {
        label: "Open Chat",
        icon: IconName::MessageSquare,
        route: Route::Chat,
        keywords: "",
        new_chat: false,
    },
    Command {
        label: "Run Benchmark",
        icon: IconName::Gauge,
        route: Route::Benchmark,
        keywords: "performance speed",
        new_chat: false,
    },
    Command {
        label: "Mesh Status",
        icon: IconName::Network,
        route: Route::Mesh,
        keywords: "p2p peer network",
        new_chat: false,
    },
    Command {
        label: "Account",
        icon: IconName::CircleUser,
        route: Route::Account,
        keywords: "login auth subscription",
        new_chat: false,
    },
    Command {
        label: "Settings",
        icon: IconName::Settings,
        route: Route::Settings,
        keywords: "preferences config",
        new_chat: false,
    },
];

/// Where the palette sends you, and whether it asked for a fresh chat.
pub struct Choice {
    pub route: Route,
    pub new_chat: bool,
}

pub struct Palette {
    query: Entity<InputState>,
    selected: usize,
    _subscription: Subscription,
}

impl EventEmitter<Choice> for Palette {}

impl Palette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Type a command..."));
        let subscription =
            cx.subscribe_in(
                &query,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => {
                        this.selected = 0;
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.pick(this.selected, cx),
                    _ => {}
                },
            );
        Self {
            query,
            selected: 0,
            _subscription: subscription,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |q, cx| q.focus(window, cx));
    }

    fn filtered(&self, cx: &App) -> Vec<&'static Command> {
        let q = self.query.read(cx).value().to_lowercase();
        COMMANDS
            .iter()
            .filter(|c| {
                q.is_empty() || c.label.to_lowercase().contains(&q) || c.keywords.contains(&q)
            })
            .collect()
    }

    fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(cmd) = self.filtered(cx).get(index) {
            cx.emit(Choice {
                route: cmd.route,
                new_chat: cmd.new_chat,
            });
        }
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filtered(cx);
        let count = filtered.len();
        self.selected = self.selected.min(count.saturating_sub(1));

        let mut list = v_flex().py_1();
        if filtered.is_empty() {
            list = list.child(
                div()
                    .py_6()
                    .text_center()
                    .text_sm()
                    .text_color(c(TEXT_MUTED))
                    .child("No results"),
            );
        }
        for (i, cmd) in filtered.into_iter().enumerate() {
            let active = i == self.selected;
            list = list.child(
                h_flex()
                    .id(cmd.label)
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .text_sm()
                    .cursor_pointer()
                    .text_color(c(if active { TEXT_PRIMARY } else { TEXT_SECONDARY }))
                    .when(active, |d| d.bg(c(SURFACE_OVERLAY)))
                    .child(Icon::new(cmd.icon).size(px(14.)).text_color(c(TEXT_MUTED)))
                    .child(cmd.label)
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.selected != i {
                            this.selected = i;
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| this.pick(i, cx))),
            );
        }

        v_flex()
            .gap_2()
            // Capture phase, so the input does not swallow the arrow keys.
            .capture_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
                match ev.keystroke.key.as_str() {
                    "down" => {
                        this.selected = (this.selected + 1).min(count.saturating_sub(1));
                        cx.stop_propagation();
                        cx.notify();
                    }
                    "up" => {
                        this.selected = this.selected.saturating_sub(1);
                        cx.stop_propagation();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Command)
                            .size(px(14.))
                            .text_color(c(TEXT_MUTED)),
                    )
                    .child(div().flex_1().child(Input::new(&self.query)))
                    .child(
                        div()
                            .text_size(px(10.))
                            .font_family("monospace")
                            .text_color(c(TEXT_MUTED))
                            .child("esc"),
                    ),
            )
            .child(div().h(px(1.)).bg(c(BORDER)))
            .child(list)
    }
}
