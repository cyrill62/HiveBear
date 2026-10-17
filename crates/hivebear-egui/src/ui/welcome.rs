//! First-run welcome screen.

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Vec2};
use egui_phosphor::regular as icon;

use hivebear_core::types::{InferenceEngine, Quantization};
use hivebear_core::ModelRecommendation;

use super::shared::Shared;
use super::theme::*;
use super::util::{format_bytes, format_download_size, format_tps};
use super::widgets::{self, Btn};
use crate::backend;
use crate::tasks::Ctx;

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

pub struct Outcome {
    pub started_install: bool,
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

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        app: Option<&Ctx>,
        shared: &mut Shared,
    ) -> Option<Outcome> {
        if let (Some(choice), Some(app)) = (self.pending_usage_choice, app) {
            self.pending_usage_choice = None;
            save_onboarding_choices(app, choice);
        }
        if self.done {
            return None;
        }

        let mut outcome = None;
        let slide = &SLIDES[self.index];
        let is_last = self.index == SLIDES.len() - 1;
        let pick = shared.recommendations.as_deref().map(first_model);

        egui::Modal::new(egui::Id::new("welcome"))
            .backdrop_color(Color32::from_black_alpha(160))
            .frame(
                Frame::new()
                    .fill(SURFACE_RAISED)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(RADIUS_XL))
                    .shadow(shadow_overlay()),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    Frame::new()
                        .fill(tint(PAW_500, 0.1))
                        .corner_radius(CornerRadius::same(RADIUS_LG))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(RichText::new(slide.glyph).size(20.0).color(PAW_500));
                        });
                    ui.add_space(8.0);
                    ui.label(RichText::new(slide.title).size(18.0).strong().color(TEXT_PRIMARY));
                    ui.add(egui::Label::new(widgets::secondary(slide.body)).wrap());
                    for bullet in slide.bullets {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(icon::CHECK).size(SMALL).color(SUCCESS));
                            ui.label(widgets::muted(*bullet));
                        });
                    }

                    if self.index == PRIVACY_SLIDE {
                        ui.add_space(8.0);
                        inset(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    Vec2::new(ui.available_width() - 48.0, 0.0),
                                    egui::Label::new(widgets::secondary(
                                        "Send anonymous usage counts (first launch, first chat, benchmark shared) — no prompts, no content, no IP stored",
                                    ).size(SMALL))
                                    .wrap(),
                                );
                                widgets::toggle(ui, &mut self.usage_events);
                            });
                        });
                    }

                    if is_last {
                        ui.add_space(8.0);
                        match app {
                            Some(app) => {
                                let p = &app.state.profile;
                                inset(ui, |ui| {
                                    hw_line(ui, "CPU", &p.cpu.model_name);
                                    hw_line(ui, "RAM", &format_bytes(p.memory.total_bytes));
                                    let gpu = p.gpus.first().map_or_else(
                                        || "None detected — CPU inference".to_string(),
                                        |g| format!("{} ({})", g.name, format_bytes(g.vram_bytes)),
                                    );
                                    hw_line(ui, "GPU", &gpu);
                                });
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    widgets::spinner(ui);
                                    ui.label(widgets::muted("Checking your hardware…"));
                                });
                            }
                        }
                        if let Some((model, is_fallback)) = &pick {
                            ui.add_space(6.0);
                            Frame::new()
                                .fill(tint(PAW_500, 0.05))
                                .stroke(Stroke::new(1.0, tint(PAW_500, 0.3)))
                                .corner_radius(CornerRadius::same(RADIUS_MD))
                                .inner_margin(Margin::symmetric(12, 10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(widgets::muted(if *is_fallback {
                                        "A small model to start with"
                                    } else {
                                        "Recommended for this device"
                                    }));
                                    ui.horizontal(|ui| {
                                        ui.label(widgets::text(&model.model_name).strong());
                                        ui.label(
                                            RichText::new(model.quantization.to_string())
                                                .font(mono(SMALL))
                                                .color(TEXT_MUTED),
                                        );
                                    });
                                    let mut line = format_download_size(model.estimated_download_bytes)
                                        .map(|s| format!("{s} download"))
                                        .unwrap_or_else(|| "Download size unknown".into());
                                    if model.estimated_tokens_per_sec > 0.0 {
                                        line.push_str(&format!(
                                            " · {} estimated",
                                            format_tps(model.estimated_tokens_per_sec)
                                        ));
                                    }
                                    ui.label(widgets::secondary(line).size(SMALL));
                                });
                        }
                    }
                });

                // Footer: progress dots and actions.
                Frame::new()
                    .fill(SURFACE)
                    .inner_margin(Margin::symmetric(24, 12))
                    .corner_radius(CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: RADIUS_XL,
                        se: RADIUS_XL,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            for i in 0..SLIDES.len() {
                                let w = if i == self.index { 24.0 } else { 6.0 };
                                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 6.0), egui::Sense::hover());
                                let color = if i == self.index { PAW_500 } else { BORDER };
                                ui.painter().rect_filled(rect, CornerRadius::same(3), color);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                match (&pick, is_last, app) {
                                    (Some((model, _)), true, Some(app)) => {
                                        let size = format_download_size(model.estimated_download_bytes)
                                            .map(|s| format!(" ({s})"))
                                            .unwrap_or_default();
                                        let label = format!("Install {}{size}", model.model_name);
                                        if Btn::primary(icon::DOWNLOAD_SIMPLE, &label).show(ui).clicked() {
                                            shared.install(app, &model.model_id, Some(model.quantization.to_string()));
                                            outcome = Some(Outcome { started_install: true });
                                        }
                                    }
                                    _ => {
                                        let label = if is_last { "Let's go" } else { "Next" };
                                        if Btn::primary(icon::ARROW_RIGHT, label).show(ui).clicked() {
                                            if is_last {
                                                outcome = Some(Outcome { started_install: false });
                                            } else {
                                                self.index += 1;
                                            }
                                        }
                                    }
                                }
                                if !is_last || pick.is_some() {
                                    let label = if is_last { "Not now" } else { "Skip" };
                                    if Btn::ghost("", label).small().show(ui).clicked() {
                                        outcome = Some(Outcome { started_install: false });
                                    }
                                }
                            });
                        });
                    });
            });

        if outcome.is_some() {
            self.done = true;
            match app {
                Some(app) => save_onboarding_choices(app, self.usage_events),
                None => self.pending_usage_choice = Some(self.usage_events),
            }
        }
        outcome
    }
}

/// Persist the welcome-screen choices, then send `first_launch` — in that
/// order, so switching usage counts off here is honoured before anything goes.
/// Also records that the crash-reporting notice has been shown: opt-out
/// reporting is only defensible if people are told.
fn save_onboarding_choices(app: &Ctx, usage_events: bool) {
    let state = std::sync::Arc::clone(&app.state);
    app.detach(async move {
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
    });
}

fn inset(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(RADIUS_MD))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn hw_line(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(widgets::muted(label));
        ui.label(widgets::muted("·"));
        ui.add(egui::Label::new(widgets::secondary(value).size(SMALL)).truncate());
    });
}
