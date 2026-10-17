//! "Update available" card, bottom-right.

use std::time::Duration;

use iced::widget::{column, container, row};
use iced::{Alignment, Element, Task};

use super::icon::{self, icon};
use super::theme::{self, *};
use super::widgets::{self, Kind};
use crate::backend::update;

pub const RECHECK: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Debug, Clone)]
pub enum Msg {
    Check,
    Checked(Result<Option<String>, String>),
    Download,
    Dismiss,
}

pub fn check() -> Task<Msg> {
    Task::perform(
        async {
            let client = reqwest::Client::new();
            update::check(&client).await
        },
        Msg::Checked,
    )
}

#[derive(Default)]
pub struct UpdateNotice {
    available: Option<String>,
    dismissed: bool,
}

impl UpdateNotice {
    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Check => return check(),
            Msg::Checked(Ok(version)) => self.available = version,
            // Non-fatal for the user, but not something to swallow: a broken
            // update check is invisible by design, and v0.1.5 sat
            // un-updatable for six months because nothing reported this.
            Msg::Checked(Err(e)) => crate::telemetry::report_error(&e, "updater"),
            Msg::Download => super::util::open_url(update::RELEASES_URL),
            Msg::Dismiss => self.dismissed = true,
        }
        Task::none()
    }

    pub fn view(&self) -> Option<Element<'_, Msg>> {
        let version = self.available.as_ref().filter(|_| !self.dismissed)?;
        Some(
            container(
                row![
                    icon(icon::DOWNLOAD_SIMPLE).size(16).color(PAW_400),
                    column![
                        widgets::strong("Update available"),
                        widgets::muted(format!("v{version}"))
                    ],
                    widgets::btn_sm(Kind::Primary, None, "Download", Some(Msg::Download)),
                    widgets::icon_button(icon::X, "Dismiss", Msg::Dismiss),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding([10, 14])
            .style(theme::toast)
            .into(),
        )
    }
}
