//! First-run welcome screen, shown in a GPUI Kit dialog.

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use hivebear_core::types::{InferenceEngine, Quantization};
use hivebear_core::ModelRecommendation;

use super::shared::Shared;
use super::theme::*;
use super::util::{format_bytes, format_download_size, format_tps};
use super::{blocking, call, rt, widgets};
use crate::backend;
use crate::state::AppState;

struct Slide {
    icon: IconName,
    title: &'static str,
    body: &'static str,
    bullets: &'static [&'static str],
}

const SLIDES: &[Slide] = &[
    Slide {
        icon: IconName::Sparkles,
        title: "Welcome to HiveBear",
        body: "Run open-source AI models on your own machine — no cloud, no subscription, no sign-up required. HiveBear finds the models that fit your hardware and lets you chat with them locally.",
        bullets: &[],
    },
    Slide {
        icon: IconName::Network,
        title: "Bigger models, together",
        body: "Your laptop can't run Llama 3 70B alone — but a few peers together can. When you join the mesh, HiveBear pairs your idle compute with other bears so everyone can run models no single device could handle.",
        bullets: &[
            "The mesh is off until you choose to join it.",
            "Your device identity is a local keypair. It never leaves your machine.",
        ],
    },
    Slide {
        icon: IconName::ShieldCheck,
        title: "If something breaks",
        body: "HiveBear sends anonymous crash reports so we can fix problems we'd otherwise never hear about. You can turn this off at any time in Settings.",
        bullets: &[
            "Never your prompts, chat history, model files or account details.",
            "No name, email or IP address — just the error and a random ID.",
        ],
    },
    Slide {
        icon: IconName::Cpu,
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

/// The welcome screen is done. Carries the usage-count choice, which the
/// shell persists — possibly later, if the backend is still starting.
pub struct Finished {
    pub started_install: bool,
    pub usage_events: bool,
}

pub struct Welcome {
    index: usize,
    usage_events: bool,
    ready: Option<(Arc<AppState>, Entity<Shared>)>,
    _observe: Option<Subscription>,
}

impl EventEmitter<Finished> for Welcome {}

impl Welcome {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self {
            index: 0,
            usage_events: true,
            ready: None,
            _observe: None,
        }
    }

    /// The backend came up: show the hardware and the recommended model.
    pub fn set_ready(
        &mut self,
        state: Arc<AppState>,
        shared: Entity<Shared>,
        cx: &mut Context<Self>,
    ) {
        self._observe = Some(cx.observe(&shared, |_, _, cx| cx.notify()));
        self.ready = Some((state, shared));
        cx.notify();
    }

    fn finish(&mut self, install: bool, cx: &mut Context<Self>) {
        let mut started_install = false;
        if install {
            if let Some((_, shared)) = &self.ready {
                shared.update(cx, |s, cx| {
                    if let Some(recs) = &s.recommendations {
                        let (model, _) = first_model(recs);
                        s.install(&model.model_id, Some(model.quantization.to_string()), cx);
                        started_install = true;
                    }
                });
            }
        }
        cx.emit(Finished {
            started_install,
            usage_events: self.usage_events,
        });
    }
}

/// Persist the welcome-screen choices, then send `first_launch` — in that
/// order, so switching usage counts off here is honoured before anything goes.
/// Also records that the crash-reporting notice has been shown: opt-out
/// reporting is only defensible if people are told.
pub fn save_onboarding_choices(state: Arc<AppState>, usage_events: bool, cx: &mut App) {
    let rt = rt(cx);
    cx.background_spawn(async move {
        let saved = blocking(rt.clone(), state, move |state| {
            if let Err(e) = backend::telemetry::acknowledge_notice() {
                tracing::debug!("Could not record the telemetry notice: {e}");
            }
            let mut config = state.config();
            if config.telemetry.usage_events != usage_events {
                config.telemetry.usage_events = usage_events;
                backend::config::save_config(state, config)?;
            }
            Ok(())
        })
        .await;
        match saved {
            Ok(()) => {
                call(rt, backend::telemetry::record_first_launch()).await;
            }
            Err(e) => tracing::warn!("Could not save the usage-count choice: {e}"),
        }
    })
    .detach();
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slide = &SLIDES[self.index];
        let is_last = self.index == SLIDES.len() - 1;
        let recs = self
            .ready
            .as_ref()
            .and_then(|(_, s)| s.read(cx).recommendations.clone());
        let pick = recs.as_deref().map(first_model);

        let mut body = v_flex()
            .gap_2()
            .child(widgets::icon_tile(
                slide.icon,
                40.,
                ca(PAW_500, 0.1),
                c(PAW_500),
            ))
            .child(
                div()
                    .pt_2()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(slide.title),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(c(TEXT_SECONDARY))
                    .child(slide.body),
            );
        for bullet in slide.bullets {
            body = body.child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(
                        Icon::new(IconName::Check)
                            .size(px(12.))
                            .text_color(c(SUCCESS)),
                    )
                    .child(widgets::muted(*bullet)),
            );
        }

        if self.index == PRIVACY_SLIDE {
            body = body.child(
                widgets::inset().mt_2().child(
                    h_flex()
                        .gap_3()
                        .justify_between()
                        .child(div().text_xs().text_color(c(TEXT_SECONDARY)).child(
                            "Send anonymous usage counts (first launch, first chat, benchmark shared) — no prompts, no content, no IP stored",
                        ))
                        .child(Switch::new("usage-events").checked(self.usage_events).on_click(cx.listener(
                            |this, checked: &bool, _, cx| {
                                this.usage_events = *checked;
                                cx.notify();
                            },
                        ))),
                ),
            );
        }

        if is_last {
            body = body.child(match &self.ready {
                Some((state, _)) => {
                    let p = &state.profile;
                    let gpu = p.gpus.first().map_or_else(
                        || "None detected — CPU inference".to_string(),
                        |g| format!("{} ({})", g.name, format_bytes(g.vram_bytes)),
                    );
                    widgets::inset()
                        .mt_2()
                        .child(hw_line("CPU", p.cpu.model_name.clone()))
                        .child(hw_line("RAM", format_bytes(p.memory.total_bytes)))
                        .child(hw_line("GPU", gpu))
                        .into_any_element()
                }
                None => widgets::muted("Checking your hardware…").into_any_element(),
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
                body = body.child(
                    widgets::accent_card()
                        .p_3()
                        .child(widgets::muted(if *is_fallback {
                            "A small model to start with"
                        } else {
                            "Recommended for this device"
                        }))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(model.model_name.clone()),
                                )
                                .child(widgets::mono(model.quantization.to_string(), TEXT_MUTED)),
                        )
                        .child(div().text_xs().text_color(c(TEXT_SECONDARY)).child(line)),
                );
            }
        }

        // Footer: progress dots and actions.
        let dots = h_flex().gap_1p5().children((0..SLIDES.len()).map(|i| {
            let on = i == self.index;
            div()
                .h(px(6.))
                .w(px(if on { 24. } else { 6. }))
                .rounded_full()
                .bg(c(if on { PAW_500 } else { BORDER }))
        }));
        let install = match (&pick, is_last, self.ready.is_some()) {
            (Some((model, _)), true, true) => {
                let size = format_download_size(model.estimated_download_bytes)
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                Some(
                    widgets::button(
                        "welcome-install",
                        format!("Install {}{size}", model.model_name),
                    )
                    .icon(IconName::Download)
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.finish(true, cx))),
                )
            }
            _ => None,
        };
        let has_install = install.is_some();
        let actions = h_flex()
            .gap_2()
            .when(!is_last || pick.is_some(), |d| {
                d.child(
                    widgets::button("welcome-skip", if is_last { "Not now" } else { "Skip" })
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.finish(false, cx))),
                )
            })
            .children(install)
            .when(!has_install, |d| {
                d.child(
                    widgets::button("welcome-next", if is_last { "Let's go" } else { "Next" })
                        .icon(IconName::ArrowRight)
                        .primary()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if is_last {
                                this.finish(false, cx);
                            } else {
                                this.index += 1;
                                cx.notify();
                            }
                        })),
                )
            });

        v_flex().gap_5().child(body).child(
            h_flex()
                .justify_between()
                .pt_3()
                .border_t_1()
                .border_color(c(BORDER))
                .child(dots)
                .child(actions),
        )
    }
}

fn hw_line(name: &'static str, value: String) -> Div {
    h_flex()
        .gap_1p5()
        .text_xs()
        .child(div().text_color(c(TEXT_MUTED)).child(name))
        .child(div().text_color(c(TEXT_MUTED)).child("·"))
        .child(div().text_color(c(TEXT_SECONDARY)).child(value))
}
