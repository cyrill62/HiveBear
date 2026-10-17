use std::time::{Duration, Instant};

use iced::widget::{column, container, row, space};
use iced::{Alignment, Color, Element, Fill, Task};

use super::dashboard::EARLY_MESSAGE;
use crate::backend::mesh::{self as mesh_backend, MeshConnectionStatus};
use crate::error::CmdResult;
use crate::ui::icon::{self, icon};
use crate::ui::theme::*;
use crate::ui::widgets::{self, Kind, Pad, Tone};
use crate::ui::{blocking, call, scroll_page, Cx, Env, Route};

/// How long to watch a fresh join for a failed bind or registration, which
/// only shows up a moment after the node starts in the background.
const JOIN_WATCH: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum Msg {
    SetEnabled(bool),
    EnabledSet(CmdResult<()>),
    Join,
    Joined(CmdResult<MeshConnectionStatus>),
    Leave,
    Left(CmdResult<()>),
    Go(Route),
}

#[derive(Default)]
pub struct Mesh {
    toggling: bool,
    joining: bool,
    leaving: bool,
    watching_join: Option<Instant>,
}

impl Mesh {
    pub fn watching(&self) -> bool {
        self.watching_join.is_some()
    }

    /// Timer tick: after a join, surface a failed start as soon as it shows.
    pub fn tick(&mut self, env: &mut Env) -> Task<Msg> {
        if let Some(started) = self.watching_join {
            let status = mesh_backend::connection_status(env.state);
            if let Some(e) = status.last_error {
                env.toasts.error(format!("Couldn't join the hive: {e}"));
                self.watching_join = None;
            } else if status.registered || started.elapsed() > JOIN_WATCH {
                self.watching_join = None;
            }
        }
        Task::none()
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            Msg::SetEnabled(enable) if !self.toggling => {
                self.toggling = true;
                return call(
                    env.state,
                    |s| async move { mesh_backend::set_mesh_enabled(&s, enable).await },
                    Msg::EnabledSet,
                );
            }
            Msg::SetEnabled(_) => {}
            Msg::EnabledSet(result) => {
                self.toggling = false;
                if let Err(e) = result {
                    env.toasts
                        .error(format!("Couldn't change the mesh setting: {e}"));
                }
            }
            Msg::Join if !self.joining => {
                self.joining = true;
                return blocking(env.state, mesh_backend::join_mesh, Msg::Joined);
            }
            Msg::Join => {}
            Msg::Joined(result) => {
                self.joining = false;
                match result {
                    Ok(_) => self.watching_join = Some(Instant::now()),
                    Err(e) => env.toasts.error(format!("Couldn't join the hive: {e}")),
                }
            }
            Msg::Leave if !self.leaving => {
                self.leaving = true;
                return call(
                    env.state,
                    |s| async move { mesh_backend::leave_mesh(&s).await },
                    Msg::Left,
                );
            }
            Msg::Leave => {}
            Msg::Left(result) => {
                self.leaving = false;
                if let Err(e) = result {
                    env.toasts.error(format!("Couldn't leave the hive: {e}"));
                }
            }
            Msg::Go(route) => env.go(route),
        }
        Task::none()
    }

    /// Both reads are in-memory, so the page is always live; the 5-second
    /// tick picks up peers arriving and heartbeats failing.
    pub fn view<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let config = cx.state.config().mesh;
        let connection = mesh_backend::connection_status(cx.state);

        let header = row![
            widgets::page_title("P2P Mesh"),
            space::horizontal(),
            widgets::btn_sm(
                Kind::Secondary,
                Some(icon::GEAR),
                "Configure",
                Some(Msg::Go(Route::Settings))
            ),
        ]
        .align_y(Alignment::Center);

        let paid = config.tier == "paid";
        let status = widgets::card(
            Pad::Lg,
            row![
                status_icon(
                    icon::GRAPH,
                    if config.enabled { SUCCESS } else { TEXT_MUTED }
                ),
                column![
                    row![
                        widgets::label(
                            if config.enabled {
                                "Mesh Enabled"
                            } else {
                                "Mesh Disabled"
                            },
                            16.0,
                            TEXT_PRIMARY
                        )
                        .font(widgets::BOLD),
                        widgets::badge(
                            if paid { "Paid" } else { "Free" },
                            if paid { Tone::Accent } else { Tone::Default }
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    widgets::muted(if config.enabled {
                        format!("Listening on port {}", config.port)
                    } else {
                        "Enable to join the mesh network and share compute.".into()
                    }),
                ]
                .spacing(2)
                .width(Fill),
                widgets::busy_btn(
                    if config.enabled {
                        Kind::Secondary
                    } else {
                        Kind::Primary
                    },
                    icon::POWER,
                    if config.enabled { "Disable" } else { "Enable" },
                    "Saving…",
                    self.toggling,
                    Some(Msg::SetEnabled(!config.enabled)),
                ),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        );

        let server = config
            .coordination_server
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();
        let stats = row![
            stat_card(icon::GRAPH, "Coordination Server", server),
            stat_card(
                icon::SHIELD,
                "Min Trust Score",
                format!("{:.0}%", config.min_reputation * 100.0)
            ),
            stat_card(
                icon::PULSE,
                "Verification Rate",
                format!("{:.0}%", config.verification_rate * 100.0)
            ),
        ]
        .spacing(16);

        let contribution = widgets::card(
            Pad::Md,
            column![
                widgets::section_title("Resource Contribution"),
                row![
                    widgets::muted("Share limit"),
                    space::horizontal(),
                    widgets::mono(
                        format!("{:.0}%", config.max_contribution_percent * 100.0),
                        SMALL,
                        TEXT_SECONDARY
                    ),
                ],
                widgets::bar(config.max_contribution_percent as f32, PAW_500, 8.0),
                widgets::muted(
                    "Maximum percentage of local resources shared with the mesh network."
                ),
            ]
            .spacing(8),
        );

        // "Connected" means the coordination server has acknowledged us, not
        // merely that a local socket is open.
        let (glyph, color) = if connection.registered {
            (icon::WIFI_HIGH, SUCCESS)
        } else if connection.running {
            (icon::WIFI_SLASH, WARNING)
        } else {
            (icon::WIFI_SLASH, TEXT_MUTED)
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
        let mut title_row = row![widgets::label(title, 16.0, TEXT_PRIMARY).font(widgets::BOLD)]
            .spacing(8)
            .align_y(Alignment::Center);
        if connection.registered {
            let n = connection.peer_count;
            title_row = title_row.push(widgets::badge(
                format!("{n} peer{}", if n == 1 { "" } else { "s" }),
                Tone::Default,
            ));
        }
        let busy = self.joining || self.leaving || self.watching_join.is_some();
        let action = if connection.running {
            widgets::busy_btn(
                Kind::Secondary,
                icon::WIFI_SLASH,
                "Leave",
                "…",
                busy,
                Some(Msg::Leave),
            )
        } else {
            widgets::busy_btn(
                Kind::Primary,
                icon::WIFI_HIGH,
                "Join",
                "…",
                busy,
                Some(Msg::Join),
            )
        };
        let connection_card = widgets::card(
            Pad::Lg,
            row![
                status_icon(glyph, color),
                column![title_row, widgets::muted(detail)]
                    .spacing(2)
                    .width(Fill),
                action,
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        );

        scroll_page(column![header, status, stats, contribution, connection_card].spacing(16))
    }
}

fn status_icon<'a>(glyph: &'a str, color: Color) -> Element<'a, Msg> {
    let fill = if color == TEXT_MUTED {
        SURFACE_OVERLAY
    } else {
        tint(color, 0.1)
    };
    widgets::icon_tile(glyph, 48.0, fill, color)
}

fn stat_card<'a>(glyph: &'a str, title: &str, value: String) -> Element<'a, Msg> {
    container(widgets::card(
        Pad::Md,
        column![
            row![
                icon(glyph).size(14).color(TEXT_MUTED),
                widgets::label(title.to_uppercase(), TINY, TEXT_MUTED)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            widgets::strong(value),
        ]
        .spacing(8),
    ))
    .width(Fill)
    .into()
}
