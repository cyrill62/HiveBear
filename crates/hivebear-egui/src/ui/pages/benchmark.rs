use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icon;
use hivebear_core::types::BenchmarkResult;

use crate::backend::benchmark::{self as bench, ModelBenchmarkResult};
use crate::error::CmdResult;
use crate::tasks::{poll_slot, Task};
use crate::ui::theme::*;
use crate::ui::util::{format_bytes, format_tps, open_url};
use crate::ui::widgets::{self, Btn, Pad, Tone};
use crate::ui::{Env, Route};

const LEADERBOARD_URL: &str = "https://hivebear.com/benchmarks";
const CLAIM_URL: &str = "https://hivebear.com/register?next=/benchmarks";

struct Reference {
    hardware: &'static str,
    ram: &'static str,
    gpu: &'static str,
    tps: f32,
}

const REFERENCE: &[Reference] = &[
    Reference {
        hardware: "Apple M3 Pro",
        ram: "36 GB",
        gpu: "Metal (18-core)",
        tps: 42.0,
    },
    Reference {
        hardware: "Apple M2",
        ram: "16 GB",
        gpu: "Metal (10-core)",
        tps: 28.5,
    },
    Reference {
        hardware: "Apple M1",
        ram: "16 GB",
        gpu: "Metal (8-core)",
        tps: 18.2,
    },
    Reference {
        hardware: "RTX 4090",
        ram: "64 GB",
        gpu: "CUDA (24 GB)",
        tps: 95.0,
    },
    Reference {
        hardware: "RTX 3080",
        ram: "32 GB",
        gpu: "CUDA (10 GB)",
        tps: 52.0,
    },
    Reference {
        hardware: "RTX 3060",
        ram: "16 GB",
        gpu: "CUDA (12 GB)",
        tps: 35.0,
    },
    Reference {
        hardware: "Intel i7-13700K",
        ram: "32 GB",
        gpu: "CPU only",
        tps: 8.5,
    },
    Reference {
        hardware: "AMD Ryzen 7 5800X",
        ram: "32 GB",
        gpu: "CPU only",
        tps: 7.2,
    },
    Reference {
        hardware: "Raspberry Pi 5",
        ram: "8 GB",
        gpu: "CPU only",
        tps: 1.8,
    },
];

pub struct Benchmark {
    model_id: String,
    duration: u32,

    /// A real benchmark of an installed model, and sharing it.
    real: Option<ModelBenchmarkResult>,
    real_task: Option<Task<CmdResult<ModelBenchmarkResult>>>,
    share_task: Option<Task<CmdResult<()>>>,
    shared: bool,

    /// The synthetic CPU estimate, for when no model is installed.
    estimate: Option<BenchmarkResult>,
    estimate_task: Option<Task<CmdResult<Option<BenchmarkResult>>>>,

    error: Option<String>,
}

impl Default for Benchmark {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            duration: 30,
            real: None,
            real_task: None,
            share_task: None,
            shared: false,
            estimate: None,
            estimate_task: None,
            error: None,
        }
    }
}

impl Benchmark {
    pub fn enter(&mut self, env: &mut Env) {
        env.shared.refresh_installed(env.ctx);
    }

    fn busy(&self) -> bool {
        self.real_task.is_some() || self.estimate_task.is_some()
    }

    pub fn poll(&mut self, env: &mut Env) {
        if let Some(result) = poll_slot(&mut self.real_task) {
            match result {
                Ok(r) => self.real = Some(r),
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = poll_slot(&mut self.estimate_task) {
            match result {
                Ok(r) => self.estimate = r,
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = poll_slot(&mut self.share_task) {
            match result {
                Ok(()) => {
                    self.shared = true;
                    env.toasts.success("Shared to the community leaderboard.");
                }
                Err(e) => env.toasts.error(e),
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        let installed = &env.shared.installed;
        if self.model_id.is_empty() || !installed.iter().any(|m| m.id == self.model_id) {
            self.model_id = installed.first().map(|m| m.id.clone()).unwrap_or_default();
        }
        let has_models = !installed.is_empty();
        let busy = self.busy();

        widgets::page_title(ui, "Benchmark");
        ui.label(widgets::secondary(
            "Measure how fast a model really runs on this machine, then put it on the community leaderboard.",
        ));

        if !env.shared.installed_loaded {
            ui.label(widgets::muted("Looking for installed models…"));
        } else if has_models {
            let mut run = false;
            widgets::card(ui, Pad::Lg, |ui| {
                ui.label(widgets::muted("Model"));
                ui.horizontal(|ui| {
                    let selected = installed
                        .iter()
                        .find(|m| m.id == self.model_id)
                        .map_or(String::new(), |m| m.name.clone());
                    ui.add_enabled_ui(!busy, |ui| {
                        egui::ComboBox::from_id_salt("bench-model")
                            .selected_text(selected)
                            .width(ui.available_width() - 180.0)
                            .show_ui(ui, |ui| {
                                for m in installed {
                                    ui.selectable_value(&mut self.model_id, m.id.clone(), &m.name);
                                }
                            });
                    });
                    let label = if self.real_task.is_some() {
                        "Running…"
                    } else {
                        "Run benchmark"
                    };
                    run = Btn::primary(icon::PLAY, label)
                        .enabled(!self.model_id.is_empty())
                        .busy(busy)
                        .show(ui)
                        .clicked();
                });
                if self.real_task.is_some() {
                    ui.horizontal(|ui| {
                        widgets::spinner(ui);
                        ui.label(widgets::muted(
                            "Loading the model and generating tokens. This usually takes 30–60 seconds.",
                        ));
                    });
                }
            });
            if run {
                self.error = None;
                self.real = None;
                self.shared = false;
                let id = self.model_id.clone();
                self.real_task = Some(
                    env.ctx
                        .run(move |s| async move { bench::run_model_benchmark(&s, id).await }),
                );
            }
        } else {
            // No model: only a clearly labelled estimate is possible.
            let mut run = false;
            let mut install = false;
            widgets::card(ui, Pad::Lg, |ui| {
                ui.horizontal(|ui| {
                    widgets::section_title(ui, "Quick estimate (no model installed)");
                    widgets::badge(ui, "Estimate", Tone::Default);
                });
                ui.add(egui::Label::new(widgets::muted(
                    "A CPU math test that guesses speed. It is not a real model run, so it can't go on the \
                     leaderboard. Install a model for a real benchmark.",
                )).wrap());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(widgets::muted("Duration (seconds)"));
                    ui.add_enabled(
                        !busy,
                        egui::DragValue::new(&mut self.duration).range(5..=120),
                    );
                    let label = if self.estimate_task.is_some() {
                        "Estimating…"
                    } else {
                        "Run quick estimate"
                    };
                    run = Btn::secondary(icon::GAUGE, label)
                        .busy(busy)
                        .show(ui)
                        .clicked();
                    install = Btn::primary(icon::DOWNLOAD_SIMPLE, "Install a model")
                        .show(ui)
                        .clicked();
                });
            });
            if run {
                self.error = None;
                self.estimate = None;
                let secs = self.duration;
                self.estimate_task = Some(env.ctx.spawn(bench::run_benchmark(secs)));
            }
            if install {
                env.go(Route::Models);
            }
        }

        if let Some(e) = &self.error {
            widgets::error_banner(ui, e);
        }

        if let Some(real) = &self.real {
            let mut share = false;
            widgets::card(ui, Pad::Lg, |ui| {
                ui.horizontal(|ui| {
                    widgets::section_title(ui, "Results");
                    widgets::badge(ui, &real.quantization, Tone::Accent);
                    widgets::badge(ui, &real.engine, Tone::Default);
                });
                ui.add_space(4.0);
                let r = &real.result;
                egui::Grid::new("bench-results")
                    .num_columns(4)
                    .spacing([32.0, 12.0])
                    .show(ui, |ui| {
                        widgets::stat(ui, "Generation", &format_tps(r.tokens_per_sec), true);
                        widgets::stat(
                            ui,
                            "Time to First Token",
                            &format!("{} ms", r.time_to_first_token_ms),
                            false,
                        );
                        let pp = r
                            .prompt_eval_tokens_per_sec
                            .map_or("N/A".into(), format_tps);
                        widgets::stat(ui, "Prompt Processing", &pp, false);
                        let mem = if r.peak_memory_bytes > 0 {
                            format_bytes(r.peak_memory_bytes)
                        } else {
                            "N/A".into()
                        };
                        widgets::stat(ui, "Peak Memory", &mem, false);
                        ui.end_row();
                        widgets::stat(
                            ui,
                            "Tokens Generated",
                            &r.tokens_generated.to_string(),
                            false,
                        );
                        widgets::stat(
                            ui,
                            "Total Duration",
                            &format!("{:.1}s", r.total_duration_ms as f64 / 1000.0),
                            false,
                        );
                        widgets::stat(ui, "Model", &real.model_id, false);
                        ui.end_row();
                    });
                ui.separator();
                if self.shared {
                    ui.label(
                        RichText::new(format!(
                            "{}  Shared to the community leaderboard.",
                            icon::CHECK
                        ))
                        .color(SUCCESS),
                    );
                    ui.horizontal(|ui| {
                        if Btn::primary(icon::ARROW_SQUARE_OUT, "See the leaderboard")
                            .small()
                            .show(ui)
                            .clicked()
                        {
                            open_url(ui.ctx(), LEADERBOARD_URL);
                        }
                        if Btn::secondary(icon::USER_PLUS, "Claim your machine with a free account")
                            .small()
                            .show(ui)
                            .clicked()
                        {
                            open_url(ui.ctx(), CLAIM_URL);
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        share = Btn::primary(icon::SHARE_NETWORK, "Share to leaderboard")
                            .busy(self.share_task.is_some())
                            .show(ui)
                            .clicked();
                        ui.add(egui::Label::new(widgets::muted(
                            "Anonymous: model, speed and a hardware class (GPU type, RAM bucket). No name, no files, no prompts.",
                        )).wrap());
                    });
                }
            });
            if share {
                let result = real.clone();
                self.share_task = Some(
                    env.ctx
                        .run(move |s| async move { bench::share_benchmark(&s, &result).await }),
                );
            }
        }

        if !has_models {
            if let Some(est) = &self.estimate {
                widgets::card(ui, Pad::Lg, |ui| {
                    ui.horizontal(|ui| {
                        widgets::section_title(ui, "Estimate");
                        widgets::badge(ui, "Not a real model run", Tone::Default);
                    });
                    egui::Grid::new("bench-estimate")
                        .num_columns(3)
                        .spacing([32.0, 12.0])
                        .show(ui, |ui| {
                            widgets::stat(
                                ui,
                                "Estimated tok/s (7B Q4)",
                                &format_tps(est.tokens_per_sec),
                                true,
                            );
                            widgets::stat(
                                ui,
                                "Total Duration",
                                &format!("{:.1}s", est.total_duration_ms as f64 / 1000.0),
                                false,
                            );
                            widgets::stat(
                                ui,
                                "CPU Utilization",
                                &format!("{:.0}% of one core", est.cpu_utilization),
                                false,
                            );
                            ui.end_row();
                        });
                });
            }
        }

        let shown_tps = self
            .real
            .as_ref()
            .map(|r| r.result.tokens_per_sec)
            .or_else(|| {
                (!has_models)
                    .then(|| self.estimate.as_ref().map(|e| e.tokens_per_sec))
                    .flatten()
            });

        widgets::card(ui, Pad::Md, |ui| {
            widgets::section_title(ui, "Reference Benchmarks");
            ui.label(widgets::muted(
                "Typical performance (Llama 3.1 8B Q4_K_M, llama.cpp). Compare like with like: a smaller model runs faster.",
            ));
            ui.add_space(4.0);
            egui::Grid::new("bench-reference")
                .num_columns(4)
                .striped(true)
                .spacing([24.0, 6.0])
                .min_col_width(80.0)
                .show(ui, |ui| {
                    for h in ["Hardware", "RAM", "GPU", "Tokens/sec"] {
                        ui.label(widgets::muted(h).strong());
                    }
                    ui.end_row();
                    for r in REFERENCE {
                        let close = shown_tps.is_some_and(|t| (t - r.tps).abs() < 5.0);
                        let color = if close { PAW_400 } else { TEXT_SECONDARY };
                        ui.label(RichText::new(r.hardware).size(SMALL).color(color));
                        ui.label(RichText::new(r.ram).font(mono(SMALL)).color(color));
                        ui.label(RichText::new(r.gpu).size(SMALL).color(color));
                        let mut v = format!("{:.1}", r.tps);
                        if close {
                            v.push_str("  ← you");
                        }
                        ui.label(RichText::new(v).font(mono(SMALL)).color(color));
                        ui.end_row();
                    }
                });
            if let Some(tps) = shown_tps {
                let verdict = if tps >= 30.0 {
                    "Excellent! Well above average."
                } else if tps >= 15.0 {
                    "Good performance for most models."
                } else if tps >= 5.0 {
                    "Usable, but larger models may be slow."
                } else {
                    "Consider using smaller quantizations or smaller models."
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(widgets::muted("Your result:"));
                    ui.label(
                        RichText::new(format!("{tps:.1} tok/s"))
                            .font(mono(SMALL))
                            .color(PAW_400),
                    );
                    ui.label(widgets::muted(format!("— {verdict}")));
                });
            }
        });

        if shown_tps.is_none() && !busy {
            widgets::empty_state(
                ui,
                icon::GAUGE,
                "See how fast your hardware runs AI",
                "Run a 30-second benchmark, then share it to see how your machine compares.",
                |_| {},
            );
        }
    }
}
