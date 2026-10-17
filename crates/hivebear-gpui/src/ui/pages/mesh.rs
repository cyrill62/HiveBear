use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex, v_flex, Icon, Sizable as _};
use gpui_kit::*;

use super::dashboard::EARLY_MESSAGE;
use crate::backend::mesh as mesh_backend;
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::widgets::{self, Tone};
use crate::ui::{blocking, cmd, rt, toast_err, Navigate, Route};

/// How long to watch a fresh join for a failed bind or registration, which
/// only shows up a moment after the node starts in the background.
const JOIN_WATCH: Duration = Duration::from_secs(5);

pub struct Mesh {
    state: Arc<AppState>,
    toggling: bool,
    joining: bool,
    leaving: bool,
    _watch: Option<Task<()>>,
}

impl EventEmitter<Navigate> for Mesh {}

impl Mesh {
    pub fn new(
        state: Arc<AppState>,
        _: Entity<Shared>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Self {
        Self {
            state,
            toggling: false,
            joining: false,
            leaving: false,
            _watch: None,
        }
    }

    pub fn enter(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn set_enabled(&mut self, enable: bool, cx: &mut Context<Self>) {
        if self.toggling {
            return;
        }
        self.toggling = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move {
                mesh_backend::set_mesh_enabled(&s, enable).await
            })
            .await;
            this.update(cx, |this, cx| {
                this.toggling = false;
                if let Err(e) = result {
                    toast_err(cx, format!("Couldn't change the mesh setting: {e}"));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn join(&mut self, cx: &mut Context<Self>) {
        if self.joining {
            return;
        }
        self.joining = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        self._watch = Some(cx.spawn(async move |this, cx| {
            let result = blocking(rt, Arc::clone(&s), mesh_backend::join_mesh).await;
            if let Err(e) = result {
                this.update(cx, |this, cx| {
                    this.joining = false;
                    toast_err(cx, format!("Couldn't join the hive: {e}"));
                    cx.notify();
                })
                .ok();
                return;
            }
            // The node starts in the background, so a failed bind or
            // registration only shows up a moment later.
            let started = Instant::now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let status = mesh_backend::connection_status(&s);
                let done = status.last_error.is_some()
                    || status.registered
                    || started.elapsed() > JOIN_WATCH;
                let alive = this
                    .update(cx, |this, cx| {
                        if let Some(e) = &status.last_error {
                            toast_err(cx, format!("Couldn't join the hive: {e}"));
                        }
                        if done {
                            this.joining = false;
                        }
                        cx.notify();
                    })
                    .is_ok();
                if done || !alive {
                    return;
                }
            }
        }));
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        if self.leaving {
            return;
        }
        self.leaving = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move { mesh_backend::leave_mesh(&s).await }).await;
            this.update(cx, |this, cx| {
                this.leaving = false;
                if let Err(e) = result {
                    toast_err(cx, format!("Couldn't leave the hive: {e}"));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for Mesh {
    /// Both reads are in-memory, so the page is always live; the shell's
    /// five-second refresh picks up peers arriving and heartbeats failing.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = self.state.config().mesh;
        let connection = mesh_backend::connection_status(&self.state);

        let header = h_flex()
            .justify_between()
            .child(widgets::page_title("P2P Mesh"))
            .child(
                widgets::button("configure", "Configure")
                    .icon(IconName::Settings)
                    .outline()
                    .small()
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(Navigate(Route::Settings)))),
            );

        let paid = config.tier == "paid";
        let enabled = config.enabled;
        let status = widgets::card().p_5().child(
            h_flex()
                .gap_4()
                .child(status_icon(
                    IconName::Network,
                    if enabled { SUCCESS } else { TEXT_MUTED },
                ))
                .child(
                    v_flex()
                        .flex_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(div().text_base().font_weight(FontWeight::MEDIUM).child(
                                    if enabled {
                                        "Mesh Enabled"
                                    } else {
                                        "Mesh Disabled"
                                    },
                                ))
                                .child(widgets::badge(
                                    if paid { "Paid" } else { "Free" },
                                    if paid { Tone::Accent } else { Tone::Default },
                                )),
                        )
                        .child(widgets::muted(if enabled {
                            format!("Listening on port {}", config.port)
                        } else {
                            "Enable to join the mesh network and share compute.".into()
                        })),
                )
                .child({
                    let b =
                        widgets::button("mesh-toggle", if enabled { "Disable" } else { "Enable" })
                            .icon(IconName::Power)
                            .loading(self.toggling)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.set_enabled(!enabled, cx)),
                            );
                    if enabled {
                        b.outline()
                    } else {
                        b.primary()
                    }
                }),
        );

        let server = config
            .coordination_server
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();
        let stats = h_flex()
            .gap_4()
            .child(stat_card(IconName::Network, "Coordination Server", server))
            .child(stat_card(
                IconName::Shield,
                "Min Trust Score",
                format!("{:.0}%", config.min_reputation * 100.0),
            ))
            .child(stat_card(
                IconName::Activity,
                "Verification Rate",
                format!("{:.0}%", config.verification_rate * 100.0),
            ));

        let contribution = widgets::card().child(
            v_flex()
                .gap_2()
                .child(widgets::section_title("Resource Contribution"))
                .child(
                    h_flex()
                        .justify_between()
                        .child(widgets::muted("Share limit"))
                        .child(widgets::mono(
                            format!("{:.0}%", config.max_contribution_percent * 100.0),
                            TEXT_SECONDARY,
                        )),
                )
                .child(widgets::bar(
                    config.max_contribution_percent as f32,
                    c(PAW_500),
                    8.,
                ))
                .child(widgets::muted(
                    "Maximum percentage of local resources shared with the mesh network.",
                )),
        );

        // "Connected" means the coordination server has acknowledged us, not
        // merely that a local socket is open.
        let (icon, color) = if connection.registered {
            (IconName::Wifi, SUCCESS)
        } else if connection.running {
            (IconName::WifiOff, WARNING)
        } else {
            (IconName::WifiOff, TEXT_MUTED)
        };
        let title = if connection.registered {
            "Connected to Hive"
        } else if connection.running {
            "Running locally — not on the hive"
        } else {
            "Not Connected"
        };
        let detail = if connection.registered && connection.peer_count == 0 {
            EARLY_MESSAGE.to_string()
        } else if connection.registered {
            let id: String = connection
                .node_id
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(12)
                .collect();
            format!("Node {id}… registered and sending heartbeats")
        } else if connection.running {
            "The coordination server is unreachable, so other peers cannot find you. Retrying in the background.".into()
        } else if let Some(e) = &connection.last_error {
            format!("Last attempt failed: {e}")
        } else {
            "Join to share idle compute with other bears. Nothing is shared until you do.".into()
        };
        let mut title_row = h_flex().gap_2().child(
            div()
                .text_base()
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        );
        if connection.registered {
            let n = connection.peer_count;
            title_row = title_row.child(widgets::badge(
                format!("{n} peer{}", if n == 1 { "" } else { "s" }),
                Tone::Default,
            ));
        }
        let busy = self.joining || self.leaving;
        let running = connection.running;
        let action = {
            let b = widgets::button("mesh-join", if running { "Leave" } else { "Join" })
                .icon(if running {
                    IconName::WifiOff
                } else {
                    IconName::Wifi
                })
                .loading(busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if running {
                        this.leave(cx)
                    } else {
                        this.join(cx)
                    }
                }));
            if running {
                b.outline()
            } else {
                b.primary()
            }
        };
        let connection_card = widgets::card().p_5().child(
            h_flex()
                .gap_4()
                .child(status_icon(icon, color))
                .child(
                    v_flex()
                        .flex_1()
                        .child(title_row)
                        .child(widgets::muted(detail)),
                )
                .child(action),
        );

        widgets::scroll_page(
            "mesh",
            v_flex()
                .gap_6()
                .child(header)
                .child(status)
                .child(stats)
                .child(contribution)
                .child(connection_card),
        )
    }
}

fn status_icon(icon: IconName, color: u32) -> Div {
    let fill = if color == TEXT_MUTED {
        c(SURFACE_OVERLAY)
    } else {
        ca(color, 0.1)
    };
    widgets::icon_tile(icon, 48., fill, c(color))
}

fn stat_card(icon: IconName, title: &'static str, value: String) -> Div {
    div().flex_1().child(
        widgets::card().child(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .text_color(c(TEXT_MUTED))
                        .child(Icon::new(icon).size(px(14.)))
                        .child(div().text_size(px(11.)).child(title.to_uppercase())),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(value),
                ),
        ),
    )
}
