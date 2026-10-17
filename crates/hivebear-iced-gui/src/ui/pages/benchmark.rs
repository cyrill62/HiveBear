use iced::widget::{column, container, pick_list, row, rule, text_input};
use iced::{Alignment, Element, Fill, Task};

use hivebear_core::types::BenchmarkResult;

use crate::backend::benchmark::{self as bench, ModelBenchmarkResult};
use crate::error::CmdResult;
use crate::ui::icon;
use crate::ui::theme::{self, *};
use crate::ui::util::{format_bytes, format_tps};
use crate::ui::widgets::{self, Kind, Pad, Tone};
use crate::ui::{call, scroll_page, Cx, Env, Route};

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

/// An installed model as the selector shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelChoice {
    id: String,
    name: String,
}

impl std::fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    SelectModel(ModelChoice),
    Duration(String),
    RunModel,
    RunEstimate,
    ModelDone(CmdResult<ModelBenchmarkResult>),
    EstimateDone(CmdResult<Option<BenchmarkResult>>),
    Share,
    Shared(CmdResult<()>),
    OpenUrl(&'static str),
    Go(Route),
}

pub struct Benchmark {
    model_id: Option<String>,
    duration: String,
    real: Option<ModelBenchmarkResult>,
    running_real: bool,
    sharing: bool,
    shared: bool,
    estimate: Option<BenchmarkResult>,
    running_estimate: bool,
    error: Option<String>,
}

impl Default for Benchmark {
    fn default() -> Self {
        Self {
            model_id: None,
            duration: "30".into(),
            real: None,
            running_real: false,
            sharing: false,
            shared: false,
            estimate: None,
            running_estimate: false,
            error: None,
        }
    }
}

impl Benchmark {
    pub fn enter(&mut self, env: &mut Env) -> Task<Msg> {
        let t = env.shared.refresh_installed(env.state);
        env.spawn(t);
        Task::none()
    }

    fn busy(&self) -> bool {
        self.running_real || self.running_estimate
    }

    /// The selected model, defaulting to the first installed one.
    fn selected<'a>(&'a self, cx: &Cx<'a>) -> Option<ModelChoice> {
        let installed = &cx.shared.installed;
        installed
            .iter()
            .find(|m| Some(&m.id) == self.model_id.as_ref())
            .or(installed.first())
            .map(|m| ModelChoice {
                id: m.id.clone(),
                name: m.name.clone(),
            })
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            // The model stays fixed while a run is in progress.
            Msg::SelectModel(choice) if !self.busy() => self.model_id = Some(choice.id),
            Msg::SelectModel(_) => {}
            Msg::Duration(d) => self.duration = d,
            Msg::RunModel => {
                let id = self
                    .model_id
                    .clone()
                    .or_else(|| env.shared.installed.first().map(|m| m.id.clone()));
                if let (Some(id), false) = (id, self.busy()) {
                    self.error = None;
                    self.real = None;
                    self.shared = false;
                    self.running_real = true;
                    return call(
                        env.state,
                        |s| async move { bench::run_model_benchmark(&s, id).await },
                        Msg::ModelDone,
                    );
                }
            }
            Msg::RunEstimate if !self.busy() => {
                self.error = None;
                self.estimate = None;
                self.running_estimate = true;
                let secs = self
                    .duration
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(30)
                    .clamp(5, 120);
                self.duration = secs.to_string();
                return Task::perform(bench::run_benchmark(secs), Msg::EstimateDone);
            }
            Msg::RunEstimate => {}
            Msg::ModelDone(result) => {
                self.running_real = false;
                match result {
                    Ok(r) => self.real = Some(r),
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::EstimateDone(result) => {
                self.running_estimate = false;
                match result {
                    Ok(r) => self.estimate = r,
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::Share => {
                if let (Some(result), false) = (self.real.clone(), self.sharing) {
                    self.sharing = true;
                    return call(
                        env.state,
                        |s| async move { bench::share_benchmark(&s, &result).await },
                        Msg::Shared,
                    );
                }
            }
            Msg::Shared(result) => {
                self.sharing = false;
                match result {
                    Ok(()) => {
                        self.shared = true;
                        env.toasts.success("Shared to the community leaderboard.");
                    }
                    Err(e) => env.toasts.error(e),
                }
            }
            Msg::OpenUrl(url) => crate::ui::util::open_url(url),
            Msg::Go(route) => env.go(route),
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let has_models = !cx.shared.installed.is_empty();
        let busy = self.busy();

        let mut page = column![
            widgets::page_title("Benchmark"),
            widgets::secondary(
                "Measure how fast a model really runs on this machine, then put it on the community leaderboard."
            ),
        ]
        .spacing(16);

        if !cx.shared.installed_loaded {
            page = page.push(widgets::muted("Looking for installed models…"));
        } else if has_models {
            let choices: Vec<ModelChoice> = cx
                .shared
                .installed
                .iter()
                .map(|m| ModelChoice {
                    id: m.id.clone(),
                    name: m.name.clone(),
                })
                .collect();
            let picker = pick_list(choices, self.selected(cx), Msg::SelectModel)
                .width(Fill)
                .padding([7, 10]);
            let mut card = column![
                widgets::muted("Model"),
                row![
                    picker,
                    widgets::busy_btn(
                        Kind::Primary,
                        icon::PLAY,
                        "Run benchmark",
                        "Running…",
                        self.running_real,
                        (!busy).then_some(Msg::RunModel)
                    ),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(6);
            if self.running_real {
                card = card.push(widgets::muted(
                    "Loading the model and generating tokens. This usually takes 30–60 seconds.",
                ));
            }
            page = page.push(widgets::card(Pad::Lg, card));
        } else {
            // No model: only a clearly labelled estimate is possible.
            page = page.push(widgets::card(
                Pad::Lg,
                column![
                    row![
                        widgets::section_title("Quick estimate (no model installed)"),
                        widgets::badge("Estimate", Tone::Default),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    widgets::muted(
                        "A CPU math test that guesses speed. It is not a real model run, so it can't go on the \
                         leaderboard. Install a model for a real benchmark."
                    ),
                    row![
                        widgets::muted("Duration (seconds)"),
                        text_input("30", &self.duration)
                            .on_input_maybe((!busy).then_some(Msg::Duration))
                            .width(64)
                            .padding([6, 8])
                            .style(theme::input),
                        widgets::busy_btn(
                            Kind::Secondary,
                            icon::GAUGE,
                            "Run quick estimate",
                            "Estimating…",
                            self.running_estimate,
                            Some(Msg::RunEstimate)
                        ),
                        widgets::btn(Kind::Primary, Some(icon::DOWNLOAD_SIMPLE), "Install a model", Some(Msg::Go(Route::Models))),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                ]
                .spacing(8),
            ));
        }

        if let Some(e) = &self.error {
            page = page.push(widgets::error_banner(e.as_str()));
        }

        if let Some(real) = &self.real {
            let r = &real.result;
            let pp = r
                .prompt_eval_tokens_per_sec
                .map_or("N/A".into(), format_tps);
            let mem = if r.peak_memory_bytes > 0 {
                format_bytes(r.peak_memory_bytes)
            } else {
                "N/A".into()
            };
            let share: Element<'a, Msg> = if self.shared {
                column![
                    widgets::label("✓ Shared to the community leaderboard.", BODY, SUCCESS),
                    row![
                        widgets::btn_sm(
                            Kind::Primary,
                            Some(icon::ARROW_SQUARE_OUT),
                            "See the leaderboard",
                            Some(Msg::OpenUrl(LEADERBOARD_URL))
                        ),
                        widgets::btn_sm(
                            Kind::Secondary,
                            Some(icon::USER_PLUS),
                            "Claim your machine with a free account",
                            Some(Msg::OpenUrl(CLAIM_URL))
                        ),
                    ]
                    .spacing(8),
                ]
                .spacing(8)
                .into()
            } else {
                row![
                    widgets::busy_btn(Kind::Primary, icon::SHARE_NETWORK, "Share to leaderboard", "Sharing…", self.sharing, Some(Msg::Share)),
                    widgets::muted(
                        "Anonymous: model, speed and a hardware class (GPU type, RAM bucket). No name, no files, no prompts."
                    ),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            };
            page = page.push(widgets::card(
                Pad::Lg,
                column![
                    row![
                        widgets::section_title("Results"),
                        widgets::badge(real.quantization.as_str(), Tone::Accent),
                        widgets::badge(real.engine.as_str(), Tone::Default),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    row![
                        widgets::stat("Generation", format_tps(r.tokens_per_sec), true),
                        widgets::stat(
                            "Time to First Token",
                            format!("{} ms", r.time_to_first_token_ms),
                            false
                        ),
                        widgets::stat("Prompt Processing", pp, false),
                        widgets::stat("Peak Memory", mem, false),
                    ],
                    row![
                        widgets::stat("Tokens Generated", r.tokens_generated.to_string(), false),
                        widgets::stat(
                            "Total Duration",
                            format!("{:.1}s", r.total_duration_ms as f64 / 1000.0),
                            false
                        ),
                        widgets::stat("Model", real.model_id.clone(), false),
                        container("").width(Fill),
                    ],
                    rule::horizontal(1),
                    share,
                ]
                .spacing(12),
            ));
        }

        if !has_models {
            if let Some(est) = &self.estimate {
                page = page.push(widgets::card(
                    Pad::Lg,
                    column![
                        row![
                            widgets::section_title("Estimate"),
                            widgets::badge("Not a real model run", Tone::Default)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        row![
                            widgets::stat(
                                "Estimated tok/s (7B Q4)",
                                format_tps(est.tokens_per_sec),
                                true
                            ),
                            widgets::stat(
                                "Total Duration",
                                format!("{:.1}s", est.total_duration_ms as f64 / 1000.0),
                                false
                            ),
                            widgets::stat(
                                "CPU Utilization",
                                format!("{:.0}% of one core", est.cpu_utilization),
                                false
                            ),
                        ],
                    ]
                    .spacing(12),
                ));
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

        let cell = |s: String, color: iced::Color, mono: bool| -> Element<'a, Msg> {
            let t = widgets::label(s, SMALL, color);
            container(if mono { t.font(widgets::MONO) } else { t })
                .width(Fill)
                .into()
        };
        let mut table = column![row![
            cell("Hardware".into(), TEXT_MUTED, false),
            cell("RAM".into(), TEXT_MUTED, false),
            cell("GPU".into(), TEXT_MUTED, false),
            cell("Tokens/sec".into(), TEXT_MUTED, false),
        ]]
        .spacing(8);
        for r in REFERENCE {
            let close = shown_tps.is_some_and(|t| (t - r.tps).abs() < 5.0);
            let color = if close { PAW_400 } else { TEXT_SECONDARY };
            let tps = if close {
                format!("{:.1}  ← you", r.tps)
            } else {
                format!("{:.1}", r.tps)
            };
            table = table.push(row![
                cell(r.hardware.into(), color, false),
                cell(r.ram.into(), color, true),
                cell(r.gpu.into(), color, false),
                cell(tps, color, true),
            ]);
        }
        let mut reference = column![
            widgets::section_title("Reference Benchmarks"),
            widgets::muted(
                "Typical performance (Llama 3.1 8B Q4_K_M, llama.cpp). Compare like with like: a smaller model runs faster."
            ),
            container(table).padding(12).style(theme::inset),
        ]
        .spacing(8);
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
            reference = reference.push(
                row![
                    widgets::muted("Your result:"),
                    widgets::mono(format!("{tps:.1} tok/s"), SMALL, PAW_400),
                    widgets::muted(format!("— {verdict}")),
                ]
                .spacing(6),
            );
        }
        page = page.push(widgets::card(Pad::Md, reference));

        if shown_tps.is_none() && !busy {
            page = page.push(widgets::empty_state(
                icon::GAUGE,
                "See how fast your hardware runs AI",
                "Run a 30-second benchmark, then share it to see how your machine compares.",
                None,
            ));
        }
        scroll_page(page)
    }
}
