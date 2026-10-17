use std::collections::{HashMap, HashSet};

use eframe::egui::{
    self, Align, Align2, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui,
};
use egui_phosphor::regular as icon;
use hivebear_core::Config;
use hivebear_registry::StorageReport;

use crate::backend;
use crate::error::CmdResult;
use crate::tasks::{poll_slot, Task};
use crate::ui::theme::*;
use crate::ui::util::{format_bytes, open_url};
use crate::ui::widgets::{self, Btn, Kind, Pad, Tone};
use crate::ui::Env;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Section {
    #[default]
    Inference,
    Discovery,
    Mesh,
    Cloud,
    Storage,
    About,
}

const SECTIONS: [Section; 6] = [
    Section::Inference,
    Section::Discovery,
    Section::Mesh,
    Section::Cloud,
    Section::Storage,
    Section::About,
];

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Inference => "Inference",
            Section::Discovery => "Discovery",
            Section::Mesh => "Mesh",
            Section::Cloud => "Cloud",
            Section::Storage => "Storage",
            Section::About => "About",
        }
    }
}

const ABOUT_LINKS: &[(&str, &str)] = &[
    ("About HiveBear", "https://hivebear.com/about"),
    ("Privacy Policy", "https://hivebear.com/privacy"),
    ("Terms of Service", "https://hivebear.com/terms"),
    (
        "Release notes & downloads",
        "https://github.com/BeckhamLabsLLC/HiveBear/releases",
    ),
];

const POPULAR_PROVIDERS: &[&str] = &["openai", "anthropic", "gemini", "mistral", "groq"];

/// (prefix, display name, environment variable)
const CLOUD_PROVIDERS: &[(&str, &str, &str)] = &[
    ("openai", "OpenAI", "OPENAI_API_KEY"),
    ("anthropic", "Anthropic", "ANTHROPIC_API_KEY"),
    ("gemini", "Google Gemini", "GEMINI_API_KEY"),
    ("mistral", "Mistral", "MISTRAL_API_KEY"),
    ("groq", "Groq", "GROQ_API_KEY"),
    ("together", "Together AI", "TOGETHER_API_KEY"),
    ("fireworks", "Fireworks AI", "FIREWORKS_API_KEY"),
    ("deepseek", "DeepSeek", "DEEPSEEK_API_KEY"),
    ("xai", "xAI (Grok)", "XAI_API_KEY"),
    ("cerebras", "Cerebras", "CEREBRAS_API_KEY"),
    ("sambanova", "SambaNova", "SAMBANOVA_API_KEY"),
    ("perplexity", "Perplexity", "PERPLEXITY_API_KEY"),
    ("cohere", "Cohere", "COHERE_API_KEY"),
    ("ai21", "AI21 Labs", "AI21_API_KEY"),
    ("openrouter", "OpenRouter", "OPENROUTER_API_KEY"),
    ("nvidia", "NVIDIA NIM", "NVIDIA_API_KEY"),
    ("azure", "Azure OpenAI", "AZURE_OPENAI_API_KEY"),
    ("cloudflare", "Cloudflare Workers AI", "CLOUDFLARE_API_KEY"),
    ("huggingface", "HuggingFace", "HF_API_KEY"),
    ("replicate", "Replicate", "REPLICATE_API_KEY"),
    ("lepton", "Lepton AI", "LEPTON_API_KEY"),
    ("moonshot", "Moonshot AI", "MOONSHOT_API_KEY"),
    ("dashscope", "Alibaba DashScope", "DASHSCOPE_API_KEY"),
    ("zhipu", "Zhipu AI (GLM)", "ZHIPU_API_KEY"),
    ("yi", "01.AI (Yi)", "YI_API_KEY"),
    ("reka", "Reka AI", "REKA_API_KEY"),
    ("baichuan", "Baichuan", "BAICHUAN_API_KEY"),
    ("minimax", "Minimax", "MINIMAX_API_KEY"),
    ("octoai", "OctoAI", "OCTOAI_API_KEY"),
    ("anyscale", "Anyscale", "ANYSCALE_API_KEY"),
];

#[derive(Default)]
pub struct Settings {
    saved: Option<Config>,
    form: Option<Config>,
    models_dir: String,
    save_task: Option<Task<CmdResult<Config>>>,
    error: Option<String>,

    active: Section,
    scroll_to: Option<Section>,

    storage: Option<StorageReport>,
    storage_task: Option<Task<CmdResult<StorageReport>>>,

    keychain_available: bool,
    configured_keys: Vec<String>,
    keys_task: Option<Task<(bool, Vec<String>)>>,
    key_inputs: HashMap<String, String>,
    key_visible: HashSet<String>,
    show_all_providers: bool,
    key_op: Option<Task<CmdResult<()>>>,
}

impl Settings {
    pub fn enter(&mut self, env: &mut Env) {
        if !self.is_dirty() {
            self.load_from(env.state().config());
        }
        self.storage_task = Some(
            env.ctx
                .run(|s| async move { backend::registry::get_storage_report(&s).await }),
        );
        self.refresh_keys(env);
    }

    fn load_from(&mut self, config: Config) {
        self.models_dir = config.models_dir.display().to_string();
        self.form = Some(config.clone());
        self.saved = Some(config);
        self.error = None;
    }

    fn refresh_keys(&mut self, env: &Env) {
        let providers: Vec<&'static str> = CLOUD_PROVIDERS.iter().map(|p| p.0).collect();
        self.keys_task = Some(env.ctx.blocking(move |_| {
            (
                backend::secrets::is_keychain_available(),
                backend::secrets::configured_providers(&providers),
            )
        }));
    }

    /// Compared the way the web app did: as serialised JSON, since `Config`
    /// has no `PartialEq`.
    fn is_dirty(&self) -> bool {
        match (&self.saved, &self.form) {
            (Some(saved), Some(form)) => {
                let mut form = form.clone();
                form.models_dir = self.models_dir.clone().into();
                serde_json::to_value(saved).ok() != serde_json::to_value(&form).ok()
            }
            _ => false,
        }
    }

    pub fn poll(&mut self, env: &mut Env) {
        if let Some(result) = poll_slot(&mut self.save_task) {
            match result {
                Ok(config) => {
                    self.load_from(config);
                    env.toasts.success("Settings saved.");
                    // Thresholds and memory limits feed the recommender.
                    env.shared.refresh_recommendations(env.ctx);
                }
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(result) = poll_slot(&mut self.storage_task) {
            match result {
                Ok(report) => self.storage = Some(report),
                Err(e) => env.toasts.error(format!("Couldn't read storage: {e}")),
            }
        }
        if let Some((available, configured)) = poll_slot(&mut self.keys_task) {
            self.keychain_available = available;
            self.configured_keys = configured;
        }
        if let Some(result) = poll_slot(&mut self.key_op) {
            match result {
                Ok(()) => env.toasts.success("API key updated."),
                Err(e) => env.toasts.error(format!("Keychain: {e}")),
            }
            self.refresh_keys(env);
        }
    }

    fn save(&mut self, env: &Env) {
        let Some(mut config) = self.form.clone() else {
            return;
        };
        config.models_dir = self.models_dir.trim().into();
        self.error = None;
        self.save_task = Some(env.ctx.blocking(move |s| {
            // Fields the backend owns may have changed since the form was
            // loaded — a sign-in, a mesh join, a minted install ID. Writing
            // the stale copies back would sign the user out.
            let live = s.config();
            config.account = live.account;
            config.mesh.auto_join = live.mesh.auto_join;
            config.telemetry.install_id = live.telemetry.install_id;
            config.telemetry.notice_shown = live.telemetry.notice_shown;
            config.cloud.api_keys = live.cloud.api_keys;
            backend::config::save_config(s, config.clone())?;
            Ok(config)
        }));
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        if self.form.is_none() {
            ui.centered_and_justified(|ui| ui.label(widgets::muted("Loading configuration...")));
            return;
        }

        egui::Panel::left("settings-nav")
            .exact_size(152.0)
            .resizable(false)
            .frame(Frame::new().fill(SURFACE).inner_margin(Margin {
                left: 24,
                right: 8,
                top: 72,
                bottom: 0,
            }))
            .show(ui, |ui| {
                for s in SECTIONS {
                    let on = s == self.active;
                    let b = egui::Button::new(RichText::new(s.label()).size(SMALL).color(if on {
                        TEXT_PRIMARY
                    } else {
                        TEXT_MUTED
                    }))
                    .fill(if on {
                        SURFACE_OVERLAY
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .frame_when_inactive(on)
                    .min_size(egui::vec2(ui.available_width(), 28.0));
                    if ui.add(b).clicked() {
                        self.active = s;
                        self.scroll_to = Some(s);
                    }
                }
            });

        let dirty = self.is_dirty();
        egui::CentralPanel::default()
            .frame(Frame::new().fill(SURFACE))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("settings")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::new()
                            .inner_margin(Margin {
                                left: 16,
                                right: 24,
                                top: 24,
                                bottom: 96,
                            })
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 16.0;
                                widgets::page_title(ui, "Settings");
                                if let Some(e) = &self.error {
                                    widgets::error_banner(ui, e);
                                }
                                for s in SECTIONS {
                                    let response = widgets::card_frame(Pad::Md)
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            widgets::section_title(ui, s.label());
                                            ui.add_space(4.0);
                                            ui.spacing_mut().item_spacing.y = 12.0;
                                            self.section(ui, env, s);
                                        })
                                        .response;
                                    if self.scroll_to == Some(s) {
                                        response.scroll_to_me(Some(Align::TOP));
                                        self.scroll_to = None;
                                    }
                                }
                            });
                    });
            });

        if dirty {
            self.save_bar(ui.ctx(), env);
        }
    }

    fn section(&mut self, ui: &mut Ui, env: &mut Env, section: Section) {
        match section {
            Section::Inference => {
                let f = self.form.as_mut().expect("form loaded");
                field(
                    ui,
                    "Default Context Length",
                    Some("Tokens per conversation turn"),
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut f.default_context_length)
                                .range(512..=131_072)
                                .speed(256),
                        );
                    },
                );
                field(
                    ui,
                    "Min Tokens/sec",
                    Some("Models below this speed are excluded"),
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut f.min_tokens_per_sec)
                                .range(0.0..=1000.0)
                                .speed(0.5),
                        );
                    },
                );
                field(
                    ui,
                    "Max Memory Usage",
                    Some("Fraction of available memory"),
                    |ui| {
                        percent_slider(ui, &mut f.max_memory_usage, 0.1..=1.0);
                    },
                );
            }
            Section::Discovery => {
                let f = self.form.as_mut().expect("form loaded");
                field(ui, "Top N Recommendations", None, |ui| {
                    ui.add(egui::DragValue::new(&mut f.top_n_recommendations).range(1..=100));
                });
                field(ui, "Models Directory", None, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.models_dir)
                            .font(mono(13.0))
                            .desired_width(360.0),
                    );
                });
                field(
                    ui,
                    "Share Benchmarks",
                    Some("Anonymously contribute to community database"),
                    |ui| {
                        widgets::toggle(ui, &mut f.share_benchmarks);
                    },
                );
                field(
                    ui,
                    "Crash Reports",
                    Some("Send anonymous crash reports so we can fix what breaks. No prompts, no chat history, no account details. Takes effect at the next launch."),
                    |ui| {
                        widgets::toggle(ui, &mut f.telemetry.enabled);
                    },
                );
                field(
                    ui,
                    "Usage Counts",
                    Some("Send anonymous usage counts (first launch, first chat, benchmark shared). No prompts, no content, no IP stored."),
                    |ui| {
                        widgets::toggle(ui, &mut f.telemetry.usage_events);
                    },
                );
            }
            Section::Mesh => {
                let m = &mut self.form.as_mut().expect("form loaded").mesh;
                field(
                    ui,
                    "Enable Mesh",
                    Some("Join P2P network to share distributed compute"),
                    |ui| {
                        widgets::toggle(ui, &mut m.enabled);
                    },
                );
                field(ui, "Port", Some("QUIC transport port (1024-65535)"), |ui| {
                    ui.add(egui::DragValue::new(&mut m.port).range(1024..=65535));
                });
                field(ui, "Coordination Server", None, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut m.coordination_server)
                            .font(mono(13.0))
                            .desired_width(360.0),
                    );
                });
                field(ui, "Max Contribution", Some("Resources to share"), |ui| {
                    percent_slider(ui, &mut m.max_contribution_percent, 0.0..=1.0);
                });
                field(
                    ui,
                    "Verification Rate",
                    Some("Fraction of tokens to spot-check"),
                    |ui| {
                        percent_slider(ui, &mut m.verification_rate, 0.0..=1.0);
                    },
                );
            }
            Section::Cloud => {
                self.cloud_keys(ui, env);
                let f = self.form.as_mut().expect("form loaded");
                field(
                    ui,
                    "Cloud Fallback",
                    Some("Fall back to cloud when local inference fails"),
                    |ui| {
                        widgets::toggle(ui, &mut f.cloud.cloud_fallback);
                    },
                );
            }
            Section::Storage => match &self.storage {
                None => {
                    ui.label(widgets::muted("Loading storage info..."));
                }
                Some(report) => {
                    ui.horizontal(|ui| {
                        ui.label(widgets::secondary("Total:"));
                        ui.label(
                            RichText::new(format_bytes(report.total_bytes))
                                .font(mono(13.0))
                                .strong(),
                        );
                        let n = report.models.len();
                        widgets::badge(
                            ui,
                            &format!("{n} model{}", if n == 1 { "" } else { "s" }),
                            Tone::Default,
                        );
                        if !report.partial_downloads.is_empty() {
                            widgets::badge(
                                ui,
                                &format!("{} partial", report.partial_downloads.len()),
                                Tone::Warning,
                            );
                        }
                        if !report.orphaned_files.is_empty() {
                            widgets::badge(
                                ui,
                                &format!("{} orphaned", report.orphaned_files.len()),
                                Tone::Danger,
                            );
                        }
                    });
                    for m in &report.models {
                        Frame::new()
                            .fill(SURFACE_OVERLAY)
                            .corner_radius(CornerRadius::same(RADIUS_MD))
                            .inner_margin(Margin::symmetric(12, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            widgets::secondary(&m.model_name).size(SMALL),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(format_bytes(m.size_bytes))
                                                .font(mono(SMALL))
                                                .color(TEXT_MUTED),
                                        );
                                    });
                                });
                            });
                    }
                }
            },
            Section::About => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::INFO).size(24.0).color(PAW_500));
                    ui.vertical(|ui| {
                        ui.label(widgets::text("HiveBear Desktop").strong());
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .font(mono(SMALL))
                                .color(TEXT_MUTED),
                        );
                    });
                });
                for (label, url) in ABOUT_LINKS {
                    let b = egui::Button::new(
                        RichText::new(format!("{label}   {}", icon::ARROW_SQUARE_OUT))
                            .color(TEXT_SECONDARY),
                    )
                    .frame_when_inactive(false);
                    if ui.add(b).clicked() {
                        open_url(ui.ctx(), url);
                    }
                }
                ui.add(egui::Label::new(widgets::muted(
                    "HiveBear is open-source software built by BeckhamLabs. Your device identity is a local Ed25519 keypair — it never leaves your machine.",
                )).wrap());
            }
        }
    }

    /// Provider keys live in the OS keychain, not the config file, and are
    /// never read back for display — a row only shows whether one is stored.
    fn cloud_keys(&mut self, ui: &mut Ui, env: &mut Env) {
        if self.keys_task.is_none() && !self.keychain_available {
            widgets::banner(
                ui,
                WARNING,
                "No OS keychain is available, so cloud API keys can't be stored securely here.",
            );
            return;
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(self.configured_keys.len().to_string())
                    .size(SMALL)
                    .strong()
                    .color(PAW_400),
            );
            ui.label(widgets::muted(format!(
                "of {} providers configured",
                CLOUD_PROVIDERS.len()
            )));
        });

        let is_set = |p: &str| self.configured_keys.iter().any(|k| k == p);
        let configured: Vec<_> = CLOUD_PROVIDERS.iter().filter(|p| is_set(p.0)).collect();
        let popular: Vec<_> = CLOUD_PROVIDERS
            .iter()
            .filter(|p| POPULAR_PROVIDERS.contains(&p.0) && !is_set(p.0))
            .collect();
        let rest: Vec<_> = CLOUD_PROVIDERS
            .iter()
            .filter(|p| !POPULAR_PROVIDERS.contains(&p.0) && !is_set(p.0))
            .collect();

        let mut op: Option<(String, Option<String>)> = None;
        for p in &configured {
            op = op.or(self.provider_row(ui, p, true));
        }
        if !popular.is_empty() {
            widgets::overline(ui, "Add Provider");
            for p in &popular {
                op = op.or(self.provider_row(ui, p, false));
            }
        }
        if !rest.is_empty() {
            let glyph = if self.show_all_providers {
                icon::CARET_DOWN
            } else {
                icon::CARET_LEFT
            };
            let label = format!(
                "{glyph}  {} {} more providers",
                if self.show_all_providers {
                    "Hide"
                } else {
                    "Show"
                },
                rest.len()
            );
            if widgets::link_button(ui, &label, TEXT_MUTED).clicked() {
                self.show_all_providers = !self.show_all_providers;
            }
            if self.show_all_providers {
                for p in &rest {
                    op = op.or(self.provider_row(ui, p, false));
                }
            }
        }

        if let (Some((provider, key)), None) = (op, &self.key_op) {
            self.key_op = Some(env.ctx.blocking(move |_| match key {
                Some(key) => backend::secrets::set_cloud_api_key(&provider, &key),
                None => backend::secrets::delete_cloud_api_key(&provider),
            }));
        }
    }

    /// One provider. Returns `(provider, Some(key))` to store a key or
    /// `(provider, None)` to remove one.
    fn provider_row(
        &mut self,
        ui: &mut Ui,
        p: &(&str, &str, &str),
        is_set: bool,
    ) -> Option<(String, Option<String>)> {
        let (prefix, name, env_var) = *p;
        let mut op = None;
        Frame::new()
            .fill(SURFACE_OVERLAY)
            .corner_radius(CornerRadius::same(RADIUS_MD))
            .inner_margin(Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (glyph, color) = if is_set {
                        (icon::CHECK, SUCCESS)
                    } else {
                        (icon::CLOUD, TEXT_MUTED)
                    };
                    ui.label(RichText::new(glyph).size(SMALL).color(color));
                    ui.add_sized(
                        [140.0, 20.0],
                        egui::Label::new(
                            RichText::new(name).size(SMALL).strong().color(TEXT_PRIMARY),
                        )
                        .truncate(),
                    );

                    let input = self.key_inputs.entry(prefix.to_string()).or_default();
                    let visible = self.key_visible.contains(prefix);
                    let hint = if is_set {
                        "Stored in keychain — enter a new key to replace"
                    } else {
                        env_var
                    };
                    ui.add(
                        egui::TextEdit::singleline(input)
                            .hint_text(RichText::new(hint).color(TEXT_MUTED))
                            .password(!visible)
                            .font(mono(SMALL))
                            .desired_width((ui.available_width() - 190.0).max(120.0)),
                    );
                    let has_input = !input.trim().is_empty();
                    if has_input {
                        let glyph = if visible { icon::EYE_SLASH } else { icon::EYE };
                        if widgets::icon_button(ui, glyph, "Show key").clicked()
                            && !self.key_visible.remove(prefix)
                        {
                            self.key_visible.insert(prefix.to_string());
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{prefix}/"))
                                .font(mono(10.0))
                                .color(TEXT_MUTED),
                        );
                        if is_set
                            && Btn::new(icon::TRASH, "")
                                .kind(Kind::Danger)
                                .small()
                                .tooltip("Remove from keychain")
                                .show(ui)
                                .clicked()
                        {
                            op = Some((prefix.to_string(), None));
                        }
                        if has_input && Btn::primary("", "Save").small().show(ui).clicked() {
                            let key = std::mem::take(
                                self.key_inputs.get_mut(prefix).expect("entry exists"),
                            );
                            op = Some((prefix.to_string(), Some(key.trim().to_string())));
                        }
                    });
                });
            });
        op
    }

    fn save_bar(&mut self, ctx: &egui::Context, env: &Env) {
        let mut save = false;
        let mut discard = false;
        egui::Area::new(egui::Id::new("settings-save-bar"))
            .anchor(Align2::CENTER_BOTTOM, [76.0, -24.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(SURFACE_RAISED)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(RADIUS_XL))
                    .inner_margin(Margin::symmetric(20, 10))
                    .shadow(shadow_floating())
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(widgets::secondary("Unsaved changes"));
                            discard = Btn::ghost("", "Discard").small().show(ui).clicked();
                            save = Btn::primary(icon::FLOPPY_DISK, "Save")
                                .small()
                                .busy(self.save_task.is_some())
                                .show(ui)
                                .clicked();
                        });
                    });
            });
        if discard {
            if let Some(saved) = self.saved.clone() {
                self.load_from(saved);
            }
        }
        if save {
            self.save(env);
        }
    }
}

fn field(ui: &mut Ui, label: &str, hint: Option<&str>, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() * 0.5).min(320.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(widgets::text(label));
            if let Some(h) = hint {
                ui.add(egui::Label::new(widgets::muted(h)).wrap());
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
}

fn percent_slider(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>) {
    ui.add(
        egui::Slider::new(value, range)
            .step_by(0.01)
            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
            .custom_parser(|s| {
                s.trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .map(|v| v / 100.0)
            }),
    );
}
