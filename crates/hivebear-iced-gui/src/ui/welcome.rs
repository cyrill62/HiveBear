//! First-run welcome screen.

use std::sync::Arc;

use iced::widget::{column, container, row, space, text, toggler};
use iced::{Alignment, Element, Fill, Task};

use hivebear_core::types::{InferenceEngine, Quantization};
use hivebear_core::ModelRecommendation;

use super::icon::{self, icon};
use super::shared::Shared;
use super::theme::{self, *};
use super::util::{format_bytes, format_download_size, format_tps};
use super::widgets::{self, Kind};
use super::Message;
use crate::backend;
use crate::state::AppState;

struct Slide {
    glyph: &'static str,
    title: &'static str,
    body: &'static str,
    bullets: &'static [&'static str],
}

const SLIDES: &[Slide] = &[
    Slide {
        glyph: icon::SPARKLE,
        title: "Welcome to HiveBear",
        body: "Run open-source AI models on your own machine — no cloud, no subscription, no sign-up required. HiveBear finds the models that fit your hardware and lets you chat with them locally.",
        bullets: &[],
    },
    Slide {
        glyph: icon::NETWORK,
        title: "Bigger models, together",
        body: "Your laptop can't run Llama 3 70B alone — but a few peers together can. When you join the mesh, HiveBear pairs your idle compute with other bears so everyone can run models no single device could handle.",
        bullets: &[
            "The mesh is off until you choose to join it.",
            "Your device identity is a local keypair. It never leaves your machine.",
        ],
    },
    Slide {
        glyph: icon::SHIELD_CHECK,
        title: "If something breaks",
        body: "HiveBear sends anonymous crash reports so we can fix problems we'd otherwise never hear about. You can turn this off at any time in Settings.",
        bullets: &[
            "Never your prompts, chat history, model files or account details.",
            "No name, email or IP address — just the error and a random ID.",
        ],
    },
    Slide {
        glyph: icon::CPU,
        title: "One quick check",
        body: "HiveBear looked at your CPU, memory and GPU to pick a model that runs well here. Your hardware details stay on this device.",
        bullets: &[],
    },
];

const PRIVACY_SLIDE: usize = 2;

/// What to offer when the recommender returns nothing (unusual hardware, a
/// very small machine, or a profile that failed). An empty first-run screen is
/// where new users give up, so there is always something to install.
///
/// Qwen 2.5 0.5B is in the built-in model database (`model_db.rs`) and runs on
/// almost anything. The size is the Q4_K_M weights estimate the recommender
/// would give (0.5B params × 4.5 bits).
pub fn fallback_model() -> ModelRecommendation {
    ModelRecommendation {
        model_id: "qwen-2.5-0.5b".into(),
        model_name: "Qwen 2.5 0.5B".into(),
        quantization: Quantization::Q4KM,
        engine: InferenceEngine::LlamaCpp,
        estimated_tokens_per_sec: 0.0,
        estimated_memory_usage_bytes: 600_000_000,
        confidence: 0.0,
        warnings: vec![],
        score: 0.0,
        community_tokens_per_sec: None,
        community_sample_count: None,
        estimated_download_bytes: 281_250_000,
    }
}

/// The model to offer a first-run user: the top recommendation, or the
/// fallback. The flag says which.
pub fn first_model(recommendations: &[ModelRecommendation]) -> (ModelRecommendation, bool) {
    match recommendations.first() {
        Some(top) => (top.clone(), false),
        None => (fallback_model(), true),
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Next,
    Skip,
    Install,
    UsageEvents(bool),
}

pub enum Outcome {
    Pending(Task<Message>),
    Finished {
        task: Task<Message>,
        started_install: bool,
    },
}

pub struct Welcome {
    done: bool,
    index: usize,
    usage_events: bool,
    /// Choices made before the backend was ready, applied once it is.
    pending_usage_choice: Option<bool>,
}

impl Welcome {
    pub fn new(onboarded: bool) -> Self {
        Self {
            done: onboarded,
            index: 0,
            usage_events: true,
            pending_usage_choice: None,
        }
    }

    /// The backend came up: apply a choice made while it was starting.
    pub fn on_ready(&mut self, state: &Arc<AppState>) -> Task<Message> {
        match self.pending_usage_choice.take() {
            Some(choice) => save_onboarding_choices(state, choice),
            None => Task::none(),
        }
    }

    pub fn update(
        &mut self,
        msg: Msg,
        state: Option<&Arc<AppState>>,
        shared: &mut Shared,
    ) -> Outcome {
        let mut task = Task::none();
        let finished_with_install = match msg {
            Msg::UsageEvents(on) => {
                self.usage_events = on;
                None
            }
            Msg::Next if self.index + 1 < SLIDES.len() => {
                self.index += 1;
                None
            }
            Msg::Next | Msg::Skip => Some(false),
            Msg::Install => {
                let pick = shared.recommendations.as_deref().map(first_model);
                match (pick, state) {
                    (Some((model, _)), Some(state)) => {
                        task = shared.install(
                            state,
                            &model.model_id,
                            Some(model.quantization.to_string()),
                        );
                        Some(true)
                    }
                    _ => None,
                }
            }
        };

        let Some(started_install) = finished_with_install else {
            return Outcome::Pending(task);
        };
        self.done = true;
        let save = match state {
            Some(state) => save_onboarding_choices(state, self.usage_events),
            None => {
                self.pending_usage_choice = Some(self.usage_events);
                Task::none()
            }
        };
        Outcome::Finished {
            task: Task::batch([task, save]),
            started_install,
        }
    }

    pub fn view<'a>(
        &'a self,
        state: Option<&'a AppState>,
        shared: &'a Shared,
    ) -> Option<Element<'a, Msg>> {
        if self.done {
            return None;
        }
        let slide = &SLIDES[self.index];
        let is_last = self.index == SLIDES.len() - 1;
        let pick = shared.recommendations.as_deref().map(first_model);

        let mut body = column![
            widgets::icon_tile(slide.glyph, 40.0, tint(PAW_500, 0.1), PAW_500),
            widgets::label(slide.title, 18.0, TEXT_PRIMARY).font(widgets::BOLD),
            widgets::secondary(slide.body),
        ]
        .spacing(10);
        for bullet in slide.bullets {
            body = body.push(
                row![
                    icon(icon::CHECK).size(SMALL).color(SUCCESS),
                    widgets::muted(*bullet)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        if self.index == PRIVACY_SLIDE {
            body = body.push(
                container(
                    row![
                        widgets::label(
                            "Send anonymous usage counts (first launch, first chat, benchmark shared) — no prompts, no content, no IP stored",
                            SMALL,
                            TEXT_SECONDARY
                        )
                        .width(Fill),
                        toggler(self.usage_events).on_toggle(Msg::UsageEvents),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .padding([10, 12])
                .style(theme::inset),
            );
        }

        if is_last {
            body = body.push(match state {
                Some(state) => {
                    let p = &state.profile;
                    let gpu = p.gpus.first().map_or_else(
                        || "None detected — CPU inference".to_string(),
                        |g| format!("{} ({})", g.name, format_bytes(g.vram_bytes)),
                    );
                    container(
                        column![
                            hw_line("CPU", p.cpu.model_name.clone()),
                            hw_line("RAM", format_bytes(p.memory.total_bytes)),
                            hw_line("GPU", gpu),
                        ]
                        .spacing(4),
                    )
                    .padding([10, 12])
                    .width(Fill)
                    .style(theme::inset)
                    .into()
                }
                None => Element::from(widgets::muted("Checking your hardware…")),
            });
            if let Some((model, is_fallback)) = &pick {
                let mut line = format_download_size(model.estimated_download_bytes)
                    .map(|s| format!("{s} download"))
                    .unwrap_or_else(|| "Download size unknown".into());
                if model.estimated_tokens_per_sec > 0.0 {
                    line.push_str(&format!(
                        " · {} estimated",
                        format_tps(model.estimated_tokens_per_sec)
                    ));
                }
                body = body.push(
                    container(
                        column![
                            widgets::muted(if *is_fallback {
                                "A small model to start with"
                            } else {
                                "Recommended for this device"
                            }),
                            row![
                                widgets::strong(model.model_name.clone()),
                                widgets::mono(model.quantization.to_string(), SMALL, TEXT_MUTED),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                            widgets::label(line, SMALL, TEXT_SECONDARY),
                        ]
                        .spacing(4),
                    )
                    .padding([10, 12])
                    .width(Fill)
                    .style(theme::accent_card),
                );
            }
        }

        // Footer: progress dots and actions.
        let mut dots = row![].spacing(6).align_y(Alignment::Center);
        for i in 0..SLIDES.len() {
            let on = i == self.index;
            dots = dots.push(
                container(space())
                    .width(if on { 24 } else { 6 })
                    .height(6)
                    .style(theme::pill(if on { PAW_500 } else { BORDER }, 3.0)),
            );
        }
        let mut actions = row![].spacing(8).align_y(Alignment::Center);
        if !is_last || pick.is_some() {
            actions = actions.push(widgets::btn_sm(
                Kind::Ghost,
                None,
                if is_last { "Not now" } else { "Skip" },
                Some(Msg::Skip),
            ));
        }
        actions = actions.push(match (&pick, is_last, state.is_some()) {
            (Some((model, _)), true, true) => {
                let size = format_download_size(model.estimated_download_bytes)
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                widgets::btn(
                    Kind::Primary,
                    Some(icon::DOWNLOAD_SIMPLE),
                    format!("Install {}{size}", model.model_name),
                    Some(Msg::Install),
                )
            }
            _ => widgets::btn(
                Kind::Primary,
                Some(icon::ARROW_RIGHT),
                if is_last { "Let's go" } else { "Next" },
                Some(Msg::Next),
            ),
        });

        Some(
            container(column![
                container(body).padding(24),
                container(row![dots, space::horizontal(), actions].align_y(Alignment::Center))
                    .padding([12, 24])
                    .style(|_| iced::widget::container::Style {
                        background: Some(SURFACE.into()),
                        border: iced::Border {
                            radius: iced::border::Radius::default().bottom(RADIUS_XL),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            ])
            .width(440)
            .style(theme::floating)
            .into(),
        )
    }
}

/// Persist the welcome-screen choices, then send `first_launch` — in that
/// order, so switching usage counts off here is honoured before anything goes.
/// Also records that the crash-reporting notice has been shown: opt-out
/// reporting is only defensible if people are told.
fn save_onboarding_choices(state: &Arc<AppState>, usage_events: bool) -> Task<Message> {
    let state = Arc::clone(state);
    Task::perform(
        async move {
            let saved = tokio::task::spawn_blocking(move || {
                if let Err(e) = backend::telemetry::acknowledge_notice() {
                    tracing::debug!("Could not record the telemetry notice: {e}");
                }
                let mut config = state.config();
                if config.telemetry.usage_events != usage_events {
                    config.telemetry.usage_events = usage_events;
                    if let Err(e) = backend::config::save_config(&state, config) {
                        tracing::warn!("Could not save the usage-count choice: {e}");
                    }
                }
            })
            .await;
            if saved.is_ok() {
                backend::telemetry::record_first_launch().await;
            }
        },
        |()| (),
    )
    .discard()
}

fn hw_line<'a>(name: &'a str, value: String) -> Element<'a, Msg> {
    row![
        widgets::muted(name),
        widgets::muted("·"),
        text(value).size(SMALL).color(TEXT_SECONDARY),
    ]
    .spacing(6)
    .into()
}
