use std::collections::{HashMap, HashSet};

use iced::widget::{button, column, container, row, slider, space, text, text_input, toggler};
use iced::{Alignment, Element, Fill, Task};

use hivebear_core::Config;
use hivebear_registry::StorageReport;

use crate::backend;
use crate::error::CmdResult;
use crate::ui::icon::{self, icon};
use crate::ui::theme::{self, *};
use crate::ui::util::{format_bytes, open_url};
use crate::ui::widgets::{self, Kind, Pad, Tone};
use crate::ui::{blocking, call, scroll_page, Cx, Env};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Section {
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

/// Numeric fields, edited as text and applied when they parse.
#[derive(Clone, Copy, Debug)]
pub enum Num {
    ContextLength,
    MinTokensPerSec,
    TopN,
    MeshPort,
}

#[derive(Clone, Copy, Debug)]
pub enum Flag {
    ShareBenchmarks,
    CrashReports,
    UsageCounts,
    MeshEnabled,
    CloudFallback,
}

#[derive(Clone, Copy, Debug)]
pub enum Fraction {
    MaxMemory,
    MaxContribution,
    VerificationRate,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Show(Section),
    Number(Num, String),
    Toggle(Flag, bool),
    Slide(Fraction, f64),
    ModelsDir(String),
    CoordinationServer(String),
    Save,
    Saved(CmdResult<Box<Config>>),
    Discard,
    Storage(CmdResult<StorageReport>),
    KeysStatus(CmdResult<(bool, Vec<String>)>),
    KeyInput(&'static str, String),
    ToggleKeyVisible(&'static str),
    StoreKey(&'static str),
    RemoveKey(&'static str),
    KeyChanged(CmdResult<()>),
    ToggleAllProviders,
    OpenUrl(&'static str),
}

#[derive(Default)]
pub struct Settings {
    saved: Option<Config>,
    form: Option<Config>,
    text: HashMap<&'static str, String>,
    models_dir: String,
    saving: bool,
    error: Option<String>,
    section: Section,

    storage: Option<StorageReport>,

    keys_checked: bool,
    keychain_available: bool,
    configured_keys: Vec<String>,
    key_inputs: HashMap<&'static str, String>,
    key_visible: HashSet<&'static str>,
    show_all_providers: bool,
}

fn num_key(n: Num) -> &'static str {
    match n {
        Num::ContextLength => "ctx",
        Num::MinTokensPerSec => "min_tps",
        Num::TopN => "top_n",
        Num::MeshPort => "port",
    }
}

impl Settings {
    pub fn enter(&mut self, env: &mut Env) -> Task<Msg> {
        if !self.is_dirty() {
            self.load_from(env.state.config());
        }
        Task::batch([
            call(
                env.state,
                |s| async move { backend::registry::get_storage_report(&s).await },
                Msg::Storage,
            ),
            self.refresh_keys(env),
        ])
    }

    fn load_from(&mut self, config: Config) {
        self.models_dir = config.models_dir.display().to_string();
        self.text = HashMap::from([
            (
                num_key(Num::ContextLength),
                config.default_context_length.to_string(),
            ),
            (
                num_key(Num::MinTokensPerSec),
                config.min_tokens_per_sec.to_string(),
            ),
            (num_key(Num::TopN), config.top_n_recommendations.to_string()),
            (num_key(Num::MeshPort), config.mesh.port.to_string()),
        ]);
        self.form = Some(config.clone());
        self.saved = Some(config);
        self.error = None;
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

    fn refresh_keys(&self, env: &Env) -> Task<Msg> {
        let providers: Vec<&'static str> = CLOUD_PROVIDERS.iter().map(|p| p.0).collect();
        blocking(
            env.state,
            move |_| {
                Ok((
                    backend::secrets::is_keychain_available(),
                    backend::secrets::configured_providers(&providers),
                ))
            },
            Msg::KeysStatus,
        )
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            Msg::Show(s) => self.section = s,
            Msg::Number(n, value) => {
                if let Some(f) = &mut self.form {
                    // Out-of-range values are caught by validation on save.
                    match n {
                        Num::ContextLength => {
                            if let Ok(v) = value.trim().parse() {
                                f.default_context_length = v;
                            }
                        }
                        Num::MinTokensPerSec => {
                            if let Ok(v) = value.trim().parse() {
                                f.min_tokens_per_sec = v;
                            }
                        }
                        Num::TopN => {
                            if let Ok(v) = value.trim().parse() {
                                f.top_n_recommendations = v;
                            }
                        }
                        Num::MeshPort => {
                            if let Ok(v) = value.trim().parse() {
                                f.mesh.port = v;
                            }
                        }
                    }
                }
                self.text.insert(num_key(n), value);
            }
            Msg::Toggle(flag, on) => {
                if let Some(f) = &mut self.form {
                    match flag {
                        Flag::ShareBenchmarks => f.share_benchmarks = on,
                        Flag::CrashReports => f.telemetry.enabled = on,
                        Flag::UsageCounts => f.telemetry.usage_events = on,
                        Flag::MeshEnabled => f.mesh.enabled = on,
                        Flag::CloudFallback => f.cloud.cloud_fallback = on,
                    }
                }
            }
            Msg::Slide(which, v) => {
                if let Some(f) = &mut self.form {
                    let v = (v * 100.0).round() / 100.0;
                    match which {
                        Fraction::MaxMemory => f.max_memory_usage = v,
                        Fraction::MaxContribution => f.mesh.max_contribution_percent = v,
                        Fraction::VerificationRate => f.mesh.verification_rate = v,
                    }
                }
            }
            Msg::ModelsDir(v) => self.models_dir = v,
            Msg::CoordinationServer(v) => {
                if let Some(f) = &mut self.form {
                    f.mesh.coordination_server = v;
                }
            }
            Msg::Save if !self.saving => {
                let Some(mut config) = self.form.clone() else {
                    return Task::none();
                };
                config.models_dir = self.models_dir.trim().into();
                self.error = None;
                self.saving = true;
                return blocking(
                    env.state,
                    move |s| {
                        // Fields the backend owns may have changed since the form
                        // was loaded — a sign-in, a mesh join, a minted install ID.
                        // Writing the stale copies back would sign the user out.
                        let live = s.config();
                        config.account = live.account;
                        config.mesh.auto_join = live.mesh.auto_join;
                        config.telemetry.install_id = live.telemetry.install_id;
                        config.telemetry.notice_shown = live.telemetry.notice_shown;
                        config.cloud.api_keys = live.cloud.api_keys;
                        backend::config::save_config(s, config.clone())?;
                        Ok(Box::new(config))
                    },
                    Msg::Saved,
                );
            }
            Msg::Save => {}
            Msg::Saved(result) => {
                self.saving = false;
                match result {
                    Ok(config) => {
                        self.load_from(*config);
                        env.toasts.success("Settings saved.");
                        // Thresholds and memory limits feed the recommender.
                        let t = env.shared.refresh_recommendations(env.state);
                        env.spawn(t);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::Discard => {
                if let Some(saved) = self.saved.clone() {
                    self.load_from(saved);
                }
            }
            Msg::Storage(result) => match result {
                Ok(report) => self.storage = Some(report),
                Err(e) => env.toasts.error(format!("Couldn't read storage: {e}")),
            },
            Msg::KeysStatus(result) => {
                self.keys_checked = true;
                if let Ok((available, configured)) = result {
                    self.keychain_available = available;
                    self.configured_keys = configured;
                }
            }
            Msg::KeyInput(provider, value) => {
                self.key_inputs.insert(provider, value);
            }
            Msg::ToggleKeyVisible(provider) => {
                if !self.key_visible.remove(provider) {
                    self.key_visible.insert(provider);
                }
            }
            Msg::StoreKey(provider) => {
                let key = self
                    .key_inputs
                    .remove(provider)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !key.is_empty() {
                    return blocking(
                        env.state,
                        move |_| backend::secrets::set_cloud_api_key(provider, &key),
                        Msg::KeyChanged,
                    );
                }
            }
            Msg::RemoveKey(provider) => {
                return blocking(
                    env.state,
                    move |_| backend::secrets::delete_cloud_api_key(provider),
                    Msg::KeyChanged,
                );
            }
            Msg::KeyChanged(result) => {
                match result {
                    Ok(()) => env.toasts.success("API key updated."),
                    Err(e) => env.toasts.error(format!("Keychain: {e}")),
                }
                return self.refresh_keys(env);
            }
            Msg::ToggleAllProviders => self.show_all_providers = !self.show_all_providers,
            Msg::OpenUrl(url) => open_url(url),
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, _cx: &Cx<'a>) -> Element<'a, Msg> {
        let Some(form) = &self.form else {
            return container(widgets::muted("Loading configuration..."))
                .center(Fill)
                .into();
        };

        let mut nav = column![].spacing(2).width(128);
        for s in SECTIONS {
            nav = nav.push(
                button(text(s.label()).size(SMALL))
                    .width(Fill)
                    .padding([6, 12])
                    .style(theme::button(Kind::Row {
                        selected: s == self.section,
                    }))
                    .on_press(Msg::Show(s)),
            );
        }

        let mut content = column![widgets::page_title("Settings")].spacing(16);
        if let Some(e) = &self.error {
            content = content.push(widgets::error_banner(e.as_str()));
        }
        content = content.push(widgets::card(
            Pad::Md,
            column![
                widgets::section_title(self.section.label()),
                self.section_body(form)
            ]
            .spacing(14),
        ));
        if self.is_dirty() {
            content = content.push(
                container(
                    row![
                        widgets::secondary("Unsaved changes"),
                        widgets::btn_sm(Kind::Ghost, None, "Discard", Some(Msg::Discard)),
                        widgets::busy_btn(
                            Kind::Primary,
                            icon::FLOPPY_DISK,
                            "Save",
                            "Saving…",
                            self.saving,
                            Some(Msg::Save)
                        ),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .padding([10, 20])
                .style(theme::floating),
            );
        }

        scroll_page(
            row![
                container(nav).padding(iced::Padding {
                    top: 48.0,
                    ..Default::default()
                }),
                content
            ]
            .spacing(24),
        )
    }

    fn number<'a>(&'a self, n: Num) -> Element<'a, Msg> {
        text_input("", self.text.get(num_key(n)).map_or("", String::as_str))
            .on_input(move |v| Msg::Number(n, v))
            .width(128)
            .padding([6, 10])
            .size(BODY)
            .style(theme::input)
            .into()
    }

    fn section_body<'a>(&'a self, f: &'a Config) -> Element<'a, Msg> {
        let toggle = |flag: Flag, on: bool| -> Element<'a, Msg> {
            toggler(on).on_toggle(move |v| Msg::Toggle(flag, v)).into()
        };
        let percent = |which: Fraction, value: f64, min: f64| -> Element<'a, Msg> {
            row![
                slider(min..=1.0, value, move |v| Msg::Slide(which, v))
                    .step(0.01)
                    .width(160),
                widgets::mono(format!("{:.0}%", value * 100.0), SMALL, TEXT_MUTED).width(40),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        };

        match self.section {
            Section::Inference => column![
                widgets::field("Default Context Length", Some("Tokens per conversation turn"), self.number(Num::ContextLength)),
                widgets::field("Min Tokens/sec", Some("Models below this speed are excluded"), self.number(Num::MinTokensPerSec)),
                widgets::field(
                    "Max Memory Usage",
                    Some("Fraction of available memory"),
                    percent(Fraction::MaxMemory, f.max_memory_usage, 0.1)
                ),
            ]
            .spacing(14)
            .into(),
            Section::Discovery => column![
                widgets::field("Top N Recommendations", None, self.number(Num::TopN)),
                widgets::field(
                    "Models Directory",
                    None,
                    widgets::input("", &self.models_dir, Msg::ModelsDir).font(widgets::MONO).width(360)
                ),
                widgets::field(
                    "Share Benchmarks",
                    Some("Anonymously contribute to community database"),
                    toggle(Flag::ShareBenchmarks, f.share_benchmarks)
                ),
                widgets::field(
                    "Crash Reports",
                    Some("Send anonymous crash reports so we can fix what breaks. No prompts, no chat history, no account details. Takes effect at the next launch."),
                    toggle(Flag::CrashReports, f.telemetry.enabled)
                ),
                widgets::field(
                    "Usage Counts",
                    Some("Send anonymous usage counts (first launch, first chat, benchmark shared). No prompts, no content, no IP stored."),
                    toggle(Flag::UsageCounts, f.telemetry.usage_events)
                ),
            ]
            .spacing(14)
            .into(),
            Section::Mesh => column![
                widgets::field(
                    "Enable Mesh",
                    Some("Join P2P network to share distributed compute"),
                    toggle(Flag::MeshEnabled, f.mesh.enabled)
                ),
                widgets::field("Port", Some("QUIC transport port (1024-65535)"), self.number(Num::MeshPort)),
                widgets::field(
                    "Coordination Server",
                    None,
                    widgets::input("", &f.mesh.coordination_server, Msg::CoordinationServer)
                        .font(widgets::MONO)
                        .width(360)
                ),
                widgets::field(
                    "Max Contribution",
                    Some("Resources to share"),
                    percent(Fraction::MaxContribution, f.mesh.max_contribution_percent, 0.0)
                ),
                widgets::field(
                    "Verification Rate",
                    Some("Fraction of tokens to spot-check"),
                    percent(Fraction::VerificationRate, f.mesh.verification_rate, 0.0)
                ),
            ]
            .spacing(14)
            .into(),
            Section::Cloud => column![
                self.cloud_keys(),
                widgets::field(
                    "Cloud Fallback",
                    Some("Fall back to cloud when local inference fails"),
                    toggle(Flag::CloudFallback, f.cloud.cloud_fallback)
                ),
            ]
            .spacing(14)
            .into(),
            Section::Storage => match &self.storage {
                None => widgets::muted("Loading storage info...").into(),
                Some(report) => {
                    let n = report.models.len();
                    let mut summary = row![
                        widgets::secondary("Total:"),
                        widgets::mono(format_bytes(report.total_bytes), 13.0, TEXT_PRIMARY),
                        widgets::badge(format!("{n} model{}", if n == 1 { "" } else { "s" }), Tone::Default),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    if !report.partial_downloads.is_empty() {
                        summary = summary.push(widgets::badge(format!("{} partial", report.partial_downloads.len()), Tone::Warning));
                    }
                    if !report.orphaned_files.is_empty() {
                        summary = summary.push(widgets::badge(format!("{} orphaned", report.orphaned_files.len()), Tone::Danger));
                    }
                    let mut col = column![summary].spacing(6);
                    for m in &report.models {
                        col = col.push(
                            container(
                                row![
                                    widgets::label(m.model_name.as_str(), SMALL, TEXT_SECONDARY),
                                    space::horizontal(),
                                    widgets::mono(format_bytes(m.size_bytes), SMALL, TEXT_MUTED),
                                ]
                                .align_y(Alignment::Center),
                            )
                            .padding([6, 12])
                            .style(theme::overlay_row),
                        );
                    }
                    col.into()
                }
            },
            Section::About => {
                let mut col = column![row![
                    icon(icon::INFO).size(24).color(PAW_500),
                    column![
                        widgets::strong("HiveBear Desktop"),
                        widgets::mono(format!("v{}", env!("CARGO_PKG_VERSION")), SMALL, TEXT_MUTED),
                    ],
                ]
                .spacing(12)
                .align_y(Alignment::Center)]
                .spacing(6);
                for (label, url) in ABOUT_LINKS {
                    col = col.push(
                        button(
                            row![text(*label).size(BODY), space::horizontal(), icon(icon::ARROW_SQUARE_OUT).size(12)]
                                .align_y(Alignment::Center),
                        )
                        .width(Fill)
                        .padding([8, 12])
                        .style(theme::button(Kind::Ghost))
                        .on_press(Msg::OpenUrl(url)),
                    );
                }
                col.push(widgets::muted(
                    "HiveBear is open-source software built by BeckhamLabs. Your device identity is a local Ed25519 keypair — it never leaves your machine.",
                ))
                .into()
            }
        }
    }

    /// Provider keys live in the OS keychain, not the config file, and are
    /// never read back for display — a row only shows whether one is stored.
    fn cloud_keys(&self) -> Element<'_, Msg> {
        if !self.keys_checked {
            return widgets::muted("Checking the keychain…").into();
        }
        if !self.keychain_available {
            return widgets::banner(
                WARNING,
                "No OS keychain is available, so cloud API keys can't be stored securely here.",
            );
        }
        let is_set = |p: &str| self.configured_keys.iter().any(|k| k == p);
        let mut col = column![row![
            widgets::label(self.configured_keys.len().to_string(), SMALL, PAW_400)
                .font(widgets::BOLD),
            widgets::muted(format!("of {} providers configured", CLOUD_PROVIDERS.len())),
        ]
        .spacing(4)]
        .spacing(4);

        for p in CLOUD_PROVIDERS.iter().filter(|p| is_set(p.0)) {
            col = col.push(self.provider_row(p, true));
        }
        let popular: Vec<_> = CLOUD_PROVIDERS
            .iter()
            .filter(|p| POPULAR_PROVIDERS.contains(&p.0) && !is_set(p.0))
            .collect();
        if !popular.is_empty() {
            col = col.push(
                container(widgets::overline("Add Provider")).padding(iced::Padding {
                    top: 8.0,
                    ..Default::default()
                }),
            );
            for p in popular {
                col = col.push(self.provider_row(p, false));
            }
        }
        let rest: Vec<_> = CLOUD_PROVIDERS
            .iter()
            .filter(|p| !POPULAR_PROVIDERS.contains(&p.0) && !is_set(p.0))
            .collect();
        if !rest.is_empty() {
            let glyph = if self.show_all_providers {
                icon::CARET_DOWN
            } else {
                icon::CARET_RIGHT
            };
            col = col.push(widgets::link(
                format!(
                    "{glyph}  {} {} more providers",
                    if self.show_all_providers {
                        "Hide"
                    } else {
                        "Show"
                    },
                    rest.len()
                ),
                TEXT_MUTED,
                Msg::ToggleAllProviders,
            ));
            if self.show_all_providers {
                for p in rest {
                    col = col.push(self.provider_row(p, false));
                }
            }
        }
        col.into()
    }

    fn provider_row(
        &self,
        p: &'static (&'static str, &'static str, &'static str),
        is_set: bool,
    ) -> Element<'_, Msg> {
        let (prefix, name, env_var) = *p;
        let input = self.key_inputs.get(prefix).map_or("", String::as_str);
        let visible = self.key_visible.contains(prefix);
        let hint = if is_set {
            "Stored in keychain — enter a new key to replace"
        } else {
            env_var
        };

        let mut r = row![
            icon(if is_set { icon::CHECK } else { icon::CLOUD })
                .size(SMALL)
                .color(if is_set { SUCCESS } else { TEXT_MUTED }),
            widgets::label(name, SMALL, TEXT_PRIMARY)
                .font(widgets::BOLD)
                .width(140),
            text_input(hint, input)
                .on_input(move |v| Msg::KeyInput(prefix, v))
                .on_submit(Msg::StoreKey(prefix))
                .secure(!visible)
                .font(widgets::MONO)
                .size(SMALL)
                .padding([4, 8])
                .style(theme::input),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if !input.trim().is_empty() {
            r = r
                .push(widgets::icon_button(
                    if visible { icon::EYE_SLASH } else { icon::EYE },
                    "Show key",
                    Msg::ToggleKeyVisible(prefix),
                ))
                .push(widgets::btn_sm(
                    Kind::Primary,
                    None,
                    "Save",
                    Some(Msg::StoreKey(prefix)),
                ));
        }
        if is_set {
            r = r.push(widgets::btn_sm(
                Kind::Danger,
                Some(icon::TRASH),
                "Remove",
                Some(Msg::RemoveKey(prefix)),
            ));
        }
        r = r.push(widgets::mono(format!("{prefix}/"), 10.0, TEXT_MUTED).width(72));
        container(r)
            .padding([6, 12])
            .style(theme::overlay_row)
            .into()
    }
}
