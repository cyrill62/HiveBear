use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use hivebear_core::types::BenchmarkResult;

use crate::backend::benchmark::{self as bench, ModelBenchmarkResult};
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::util::{format_bytes, format_tps, open_url};
use crate::ui::widgets::{self, Tone};
use crate::ui::{cmd, rt, toast_err, toast_ok, Navigate, Route};

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
#[derive(Clone)]
struct ModelChoice {
    id: String,
    name: SharedString,
}

impl SelectItem for ModelChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone()
    }

    fn value(&self) -> &String {
        &self.id
    }
}

pub struct Benchmark {
    state: Arc<AppState>,
    shared: Entity<Shared>,
    picker: Entity<SelectState<SearchableVec<ModelChoice>>>,
    picker_ids: Vec<String>,
    model_id: Option<String>,
    duration: Entity<InputState>,

    real: Option<ModelBenchmarkResult>,
    running_real: bool,
    sharing: bool,
    shared_ok: bool,
    estimate: Option<BenchmarkResult>,
    running_estimate: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Navigate> for Benchmark {}

impl Benchmark {
    pub fn new(
        state: Arc<AppState>,
        shared: Entity<Shared>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<ModelChoice>::new()),
                None,
                window,
                cx,
            )
        });
        let duration = cx.new(|cx| InputState::new(window, cx).default_value("30"));
        let _subscriptions = vec![
            cx.observe_in(&shared, window, |this, _, window, cx| {
                this.sync_picker(window, cx)
            }),
            cx.subscribe(
                &picker,
                |this, _, event: &SelectEvent<SearchableVec<ModelChoice>>, cx| {
                    let SelectEvent::Confirm(value) = event;
                    // The model stays fixed while a run is in progress.
                    if !this.busy() {
                        this.model_id = value.clone();
                        cx.notify();
                    }
                },
            ),
        ];
        Self {
            state,
            shared,
            picker,
            picker_ids: vec![],
            model_id: None,
            duration,
            real: None,
            running_real: false,
            sharing: false,
            shared_ok: false,
            estimate: None,
            running_estimate: false,
            error: None,
            _subscriptions,
        }
    }

    pub fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shared.update(cx, |s, cx| s.refresh_installed(cx));
        self.sync_picker(window, cx);
    }

    /// Keep the selector's items in step with what is installed.
    fn sync_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let installed: Vec<ModelChoice> = self
            .shared
            .read(cx)
            .installed
            .iter()
            .map(|m| ModelChoice {
                id: m.id.clone(),
                name: m.name.clone().into(),
            })
            .collect();
        let ids: Vec<String> = installed.iter().map(|m| m.id.clone()).collect();
        if ids != self.picker_ids {
            if self.model_id.as_ref().is_none_or(|id| !ids.contains(id)) {
                self.model_id = ids.first().cloned();
            }
            let selected = self.model_id.clone();
            self.picker.update(cx, |p, cx| {
                p.set_items(SearchableVec::new(installed), window, cx);
                if let Some(id) = &selected {
                    p.set_selected_value(id, window, cx);
                }
            });
            self.picker_ids = ids;
        }
        cx.notify();
    }

    fn busy(&self) -> bool {
        self.running_real || self.running_estimate
    }

    fn run_model(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.model_id.clone() else {
            return;
        };
        if self.busy() {
            return;
        }
        self.error = None;
        self.real = None;
        self.shared_ok = false;
        self.running_real = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move { bench::run_model_benchmark(&s, id).await }).await;
            this.update(cx, |this, cx| {
                this.running_real = false;
                match result {
                    Ok(r) => this.real = Some(r),
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn run_estimate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let secs = self
            .duration
            .read(cx)
            .value()
            .trim()
            .parse::<u32>()
            .unwrap_or(30)
            .clamp(5, 120);
        self.duration
            .update(cx, |d, cx| d.set_value(secs.to_string(), window, cx));
        self.error = None;
        self.estimate = None;
        self.running_estimate = true;
        cx.notify();
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, bench::run_benchmark(secs)).await;
            this.update(cx, |this, cx| {
                this.running_estimate = false;
                match result {
                    Ok(r) => this.estimate = r,
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn share(&mut self, cx: &mut Context<Self>) {
        let Some(result) = self.real.clone() else {
            return;
        };
        if self.sharing {
            return;
        }
        self.sharing = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let outcome = cmd(rt, async move { bench::share_benchmark(&s, &result).await }).await;
            this.update(cx, |this, cx| {
                this.sharing = false;
                match outcome {
                    Ok(()) => {
                        this.shared_ok = true;
                        toast_ok(cx, "Shared to the community leaderboard.");
                    }
                    Err(e) => toast_err(cx, e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for Benchmark {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (has_models, installed_loaded) = {
            let s = self.shared.read(cx);
            (!s.installed.is_empty(), s.installed_loaded)
        };
        let busy = self.busy();

        let mut page = v_flex()
            .gap_6()
            .child(
                v_flex().gap_1().child(widgets::page_title("Benchmark")).child(widgets::secondary(
                    "Measure how fast a model really runs on this machine, then put it on the community leaderboard.",
                )),
            );

        if !installed_loaded {
            page = page.child(widgets::muted("Looking for installed models…"));
        } else if has_models {
            page = page.child(
                widgets::card()
                    .p_5()
                    .child(
                        h_flex()
                            .items_end()
                            .gap_4()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .child(widgets::muted("Model"))
                                    .child(Select::new(&self.picker).disabled(busy)),
                            )
                            .child(
                                widgets::button("run-benchmark", if self.running_real { "Running…" } else { "Run benchmark" })
                                    .icon(IconName::Play)
                                    .primary()
                                    .loading(self.running_real)
                                    .disabled(busy || self.model_id.is_none())
                                    .on_click(cx.listener(|this, _, _, cx| this.run_model(cx))),
                            ),
                    )
                    .when(self.running_real, |d| {
                        d.child(div().pt_3().child(widgets::muted(
                            "Loading the model and generating tokens. This usually takes 30–60 seconds.",
                        )))
                    }),
            );
        } else {
            // No model: only a clearly labelled estimate is possible.
            page = page.child(
                widgets::card()
                    .p_5()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(widgets::section_title("Quick estimate (no model installed)"))
                            .child(widgets::badge("Estimate", Tone::Default)),
                    )
                    .child(div().pt_1().child(widgets::muted(
                        "A CPU math test that guesses speed. It is not a real model run, so it can't go on the \
                         leaderboard. Install a model for a real benchmark.",
                    )))
                    .child(
                        h_flex()
                            .pt_4()
                            .items_end()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(widgets::muted("Duration (seconds)"))
                                    .child(div().w(px(96.)).child(Input::new(&self.duration).disabled(busy))),
                            )
                            .child(
                                widgets::button("run-estimate", if self.running_estimate { "Estimating…" } else { "Run quick estimate" })
                                    .icon(IconName::Gauge)
                                    .outline()
                                    .loading(self.running_estimate)
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _, window, cx| this.run_estimate(window, cx))),
                            )
                            .child(
                                widgets::button("install-a-model", "Install a model")
                                    .icon(IconName::Download)
                                    .primary()
                                    .on_click(cx.listener(|_, _, _, cx| cx.emit(Navigate(Route::Models)))),
                            ),
                    ),
            );
        }

        if let Some(e) = &self.error {
            page = page.child(widgets::banner(DANGER, e.clone()));
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
            let share = if self.shared_ok {
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .text_color(c(SUCCESS))
                            .child(gpui_kit::component::Icon::new(IconName::Check).size(px(14.)))
                            .child("Shared to the community leaderboard."),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                widgets::button("leaderboard", "See the leaderboard")
                                    .icon(IconName::ExternalLink)
                                    .primary()
                                    .small()
                                    .on_click(|_, _, _| open_url(LEADERBOARD_URL)),
                            )
                            .child(
                                widgets::button("claim", "Claim your machine with a free account")
                                    .icon(IconName::UserPlus)
                                    .outline()
                                    .small()
                                    .on_click(|_, _, _| open_url(CLAIM_URL)),
                            ),
                    )
                    .into_any_element()
            } else {
                h_flex()
                    .gap_3()
                    .child(
                        widgets::button("share", "Share to leaderboard")
                            .icon(IconName::Share2)
                            .primary()
                            .loading(self.sharing)
                            .on_click(cx.listener(|this, _, _, cx| this.share(cx))),
                    )
                    .child(widgets::muted(
                        "Anonymous: model, speed and a hardware class (GPU type, RAM bucket). No name, no files, no prompts.",
                    ))
                    .into_any_element()
            };
            page = page.child(
                widgets::card()
                    .p_5()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(widgets::section_title("Results"))
                            .child(widgets::badge(real.quantization.clone(), Tone::Accent))
                            .child(widgets::badge(real.engine.clone(), Tone::Default)),
                    )
                    .child(
                        h_flex()
                            .pt_4()
                            .gap_4()
                            .child(widgets::stat(
                                "Generation",
                                format_tps(r.tokens_per_sec),
                                true,
                            ))
                            .child(widgets::stat(
                                "Time to First Token",
                                format!("{} ms", r.time_to_first_token_ms),
                                false,
                            ))
                            .child(widgets::stat("Prompt Processing", pp, false))
                            .child(widgets::stat("Peak Memory", mem, false)),
                    )
                    .child(
                        h_flex()
                            .pt_4()
                            .gap_4()
                            .child(widgets::stat(
                                "Tokens Generated",
                                r.tokens_generated.to_string(),
                                false,
                            ))
                            .child(widgets::stat(
                                "Total Duration",
                                format!("{:.1}s", r.total_duration_ms as f64 / 1000.0),
                                false,
                            ))
                            .child(widgets::stat("Model", real.model_id.clone(), false))
                            .child(div().flex_1()),
                    )
                    .child(
                        div()
                            .mt_5()
                            .pt_4()
                            .border_t_1()
                            .border_color(c(BORDER))
                            .child(share),
                    ),
            );
        }

        if !has_models {
            if let Some(est) = &self.estimate {
                page = page.child(
                    widgets::card()
                        .p_5()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(widgets::section_title("Estimate"))
                                .child(widgets::badge("Not a real model run", Tone::Default)),
                        )
                        .child(
                            h_flex()
                                .pt_4()
                                .gap_4()
                                .child(widgets::stat(
                                    "Estimated tok/s (7B Q4)",
                                    format_tps(est.tokens_per_sec),
                                    true,
                                ))
                                .child(widgets::stat(
                                    "Total Duration",
                                    format!("{:.1}s", est.total_duration_ms as f64 / 1000.0),
                                    false,
                                ))
                                .child(widgets::stat(
                                    "CPU Utilization",
                                    format!("{:.0}% of one core", est.cpu_utilization),
                                    false,
                                )),
                        ),
                );
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

        let cell = |text: String, color: u32, mono: bool| {
            div()
                .flex_1()
                .text_xs()
                .text_color(c(color))
                .when(mono, |d| d.font_family("monospace"))
                .child(text)
        };
        let mut table = v_flex().gap_2().child(
            h_flex()
                .child(cell("Hardware".into(), TEXT_MUTED, false))
                .child(cell("RAM".into(), TEXT_MUTED, false))
                .child(cell("GPU".into(), TEXT_MUTED, false))
                .child(cell("Tokens/sec".into(), TEXT_MUTED, false)),
        );
        for r in REFERENCE {
            let close = shown_tps.is_some_and(|t| (t - r.tps).abs() < 5.0);
            let color = if close { PAW_400 } else { TEXT_SECONDARY };
            let tps = if close {
                format!("{:.1}  ← you", r.tps)
            } else {
                format!("{:.1}", r.tps)
            };
            table = table.child(
                h_flex()
                    .when(close, |d| d.bg(ca(PAW_500, 0.05)))
                    .child(cell(r.hardware.into(), color, false))
                    .child(cell(r.ram.into(), color, true))
                    .child(cell(r.gpu.into(), color, false))
                    .child(cell(tps, color, true)),
            );
        }
        let verdict = shown_tps.map(|tps| {
            let text = if tps >= 30.0 {
                "Excellent! Well above average."
            } else if tps >= 15.0 {
                "Good performance for most models."
            } else if tps >= 5.0 {
                "Usable, but larger models may be slow."
            } else {
                "Consider using smaller quantizations or smaller models."
            };
            h_flex()
                .gap_1p5()
                .child(widgets::muted("Your result:"))
                .child(widgets::mono(format!("{tps:.1} tok/s"), PAW_400))
                .child(widgets::muted(format!("— {text}")))
        });
        page = page.child(
            widgets::card()
                .child(widgets::section_title("Reference Benchmarks"))
                .child(div().pt_1().child(widgets::muted(
                    "Typical performance (Llama 3.1 8B Q4_K_M, llama.cpp). Compare like with like: a smaller model runs faster.",
                )))
                .child(div().pt_4().child(widgets::inset().p_3().child(table)))
                .children(verdict.map(|v| div().pt_3().child(v))),
        );

        if shown_tps.is_none() && !busy {
            page = page.child(widgets::empty_state(
                IconName::Gauge,
                "See how fast your hardware runs AI",
                "Run a 30-second benchmark, then share it to see how your machine compares.",
            ));
        }
        widgets::scroll_page("benchmark", page)
    }
}
