use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use hivebear_core::Config;
use hivebear_registry::StorageReport;

use crate::backend;
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::util::{format_bytes, open_url};
use crate::ui::widgets::{self, Tone};
use crate::ui::{blocking, cmd, rt, toast_err, toast_ok, Navigate};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
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

/// A text field bound to one config value.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    ContextLength,
    MinTokensPerSec,
    TopN,
    ModelsDir,
    MeshPort,
    CoordinationServer,
}

const FIELDS: [Field; 6] = [
    Field::ContextLength,
    Field::MinTokensPerSec,
    Field::TopN,
    Field::ModelsDir,
    Field::MeshPort,
    Field::CoordinationServer,
];

impl Field {
    fn read(self, c: &Config) -> String {
        match self {
            Field::ContextLength => c.default_context_length.to_string(),
            Field::MinTokensPerSec => c.min_tokens_per_sec.to_string(),
            Field::TopN => c.top_n_recommendations.to_string(),
            Field::ModelsDir => c.models_dir.display().to_string(),
            Field::MeshPort => c.mesh.port.to_string(),
            Field::CoordinationServer => c.mesh.coordination_server.clone(),
        }
    }

    /// Apply typed text. Numbers apply once they parse; out-of-range values
    /// are caught by validation on save.
    fn write(self, c: &mut Config, text: &str) {
        let t = text.trim();
        match self {
            Field::ContextLength => {
                if let Ok(v) = t.parse() {
                    c.default_context_length = v;
                }
            }
            Field::MinTokensPerSec => {
                if let Ok(v) = t.parse() {
                    c.min_tokens_per_sec = v;
                }
            }
            Field::TopN => {
                if let Ok(v) = t.parse() {
                    c.top_n_recommendations = v;
                }
            }
            Field::ModelsDir => c.models_dir = t.into(),
            Field::MeshPort => {
                if let Ok(v) = t.parse() {
                    c.mesh.port = v;
                }
            }
            Field::CoordinationServer => c.mesh.coordination_server = t.to_string(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Fraction {
    MaxMemory,
    MaxContribution,
    VerificationRate,
}

impl Fraction {
    fn read(self, c: &Config) -> f64 {
        match self {
            Fraction::MaxMemory => c.max_memory_usage,
            Fraction::MaxContribution => c.mesh.max_contribution_percent,
            Fraction::VerificationRate => c.mesh.verification_rate,
        }
    }

    fn write(self, c: &mut Config, v: f64) {
        let v = (v * 100.).round() / 100.;
        match self {
            Fraction::MaxMemory => c.max_memory_usage = v,
            Fraction::MaxContribution => c.mesh.max_contribution_percent = v,
            Fraction::VerificationRate => c.mesh.verification_rate = v,
        }
    }
}

pub struct Settings {
    state: Arc<AppState>,
    shared: Entity<Shared>,
    saved: Option<Config>,
    form: Option<Config>,
    inputs: Vec<(Field, Entity<InputState>)>,
    sliders: Vec<(Fraction, Entity<SliderState>)>,
    saving: bool,
    error: Option<String>,
    section: Section,

    storage: Option<StorageReport>,

    keys_checked: bool,
    keychain_available: bool,
    configured_keys: Vec<String>,
    key_inputs: HashMap<&'static str, Entity<InputState>>,
    key_visible: Vec<&'static str>,
    show_all_providers: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Navigate> for Settings {}

impl Settings {
    pub fn new(
        state: Arc<AppState>,
        shared: Entity<Shared>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = vec![];

        let inputs: Vec<(Field, Entity<InputState>)> = FIELDS
            .iter()
            .map(|&field| {
                let input = cx.new(|cx| InputState::new(window, cx));
                subscriptions.push(cx.subscribe(
                    &input,
                    move |this: &mut Self, input, event: &InputEvent, cx| {
                        if let InputEvent::Change = event {
                            let text = input.read(cx).value().to_string();
                            if let Some(form) = &mut this.form {
                                field.write(form, &text);
                            }
                            cx.notify();
                        }
                    },
                ));
                (field, input)
            })
            .collect();

        let sliders: Vec<(Fraction, Entity<SliderState>)> = [
            (Fraction::MaxMemory, 0.1),
            (Fraction::MaxContribution, 0.0),
            (Fraction::VerificationRate, 0.0),
        ]
        .into_iter()
        .map(|(fraction, min)| {
            let slider = cx.new(|_| SliderState::new().min(min).max(1.).step(0.01));
            subscriptions.push(cx.subscribe(
                &slider,
                move |this: &mut Self, _, event: &SliderEvent, cx| {
                    if let SliderEvent::Change(v) = event {
                        if let Some(form) = &mut this.form {
                            fraction.write(form, v.start() as f64);
                        }
                        cx.notify();
                    }
                },
            ));
            (fraction, slider)
        })
        .collect();

        // Provider keys are typed into masked fields and go straight to the
        // keychain; they are never read back for display.
        let key_inputs: HashMap<&'static str, Entity<InputState>> = CLOUD_PROVIDERS
            .iter()
            .map(|&(prefix, _, env_var)| {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(env_var)
                        .masked(true)
                });
                subscriptions.push(cx.subscribe(
                    &input,
                    move |this: &mut Self, _, event: &InputEvent, cx| match event {
                        InputEvent::Change => cx.notify(),
                        InputEvent::PressEnter { .. } => this.store_key(prefix, cx),
                        _ => {}
                    },
                ));
                (prefix, input)
            })
            .collect();

        Self {
            state,
            shared,
            saved: None,
            form: None,
            inputs,
            sliders,
            saving: false,
            error: None,
            section: Section::Inference,
            storage: None,
            keys_checked: false,
            keychain_available: false,
            configured_keys: vec![],
            key_inputs,
            key_visible: vec![],
            show_all_providers: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_dirty() {
            self.load_from(self.state.config(), window, cx);
        }
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let report = cmd(rt, async move {
                backend::registry::get_storage_report(&s).await
            })
            .await;
            this.update(cx, |this, cx| {
                match report {
                    Ok(r) => this.storage = Some(r),
                    Err(e) => toast_err(cx, format!("Couldn't read storage: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.refresh_keys(cx);
    }

    fn load_from(&mut self, config: Config, window: &mut Window, cx: &mut Context<Self>) {
        for (field, input) in &self.inputs {
            let text = field.read(&config);
            input.update(cx, |i, cx| i.set_value(text, window, cx));
        }
        for (fraction, slider) in &self.sliders {
            let v = fraction.read(&config) as f32;
            slider.update(cx, |s, cx| s.set_value(v, window, cx));
        }
        self.form = Some(config.clone());
        self.saved = Some(config);
        self.error = None;
        cx.notify();
    }

    /// Compared the way the web app did: as serialised JSON, since `Config`
    /// has no `PartialEq`.
    fn is_dirty(&self) -> bool {
        match (&self.saved, &self.form) {
            (Some(saved), Some(form)) => {
                serde_json::to_value(saved).ok() != serde_json::to_value(form).ok()
            }
            _ => false,
        }
    }

    fn toggle(&mut self, f: impl FnOnce(&mut Config, bool), on: bool, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.form {
            f(form, on);
        }
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(mut config) = self.form.clone() else {
            return;
        };
        if self.saving {
            return;
        }
        self.saving = true;
        self.error = None;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = blocking(rt, s, move |s| {
                // Fields the backend owns may have changed since the form was
                // loaded — a sign-in, a mesh join, a minted install ID.
                // Writing the stale copies back would sign the user out.
                let live = s.config();
                config.account = live.account;
                config.mesh.auto_join = live.mesh.auto_join;
                config.telemetry.install_id = live.telemetry.install_id;
                config.telemetry.notice_shown = live.telemetry.notice_shown;
                config.cloud.api_keys = live.cloud.api_keys;
                backend::config::save_config(s, config.clone())?;
                Ok(config)
            })
            .await;
            this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(config) => {
                        // The inputs already show these values; only the
                        // baseline for "unsaved changes" moves.
                        this.form = Some(config.clone());
                        this.saved = Some(config);
                        toast_ok(cx, "Settings saved.");
                        // Thresholds and memory limits feed the recommender.
                        this.shared
                            .update(cx, |s, cx| s.refresh_recommendations(cx));
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_keys(&mut self, cx: &mut Context<Self>) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        let providers: Vec<&'static str> = CLOUD_PROVIDERS.iter().map(|p| p.0).collect();
        cx.spawn(async move |this, cx| {
            let result = blocking(rt, s, move |_| {
                Ok((
                    backend::secrets::is_keychain_available(),
                    backend::secrets::configured_providers(&providers),
                ))
            })
            .await;
            this.update(cx, |this, cx| {
                this.keys_checked = true;
                if let Ok((available, configured)) = result {
                    this.keychain_available = available;
                    this.configured_keys = configured;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Store `key` for `provider` in the keychain, or remove the stored key
    /// when `key` is `None`.
    fn key_op(&mut self, provider: &'static str, key: Option<String>, cx: &mut Context<Self>) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = blocking(rt, s, move |_| match key {
                Some(key) => backend::secrets::set_cloud_api_key(provider, &key),
                None => backend::secrets::delete_cloud_api_key(provider),
            })
            .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => toast_ok(cx, "API key updated."),
                    Err(e) => toast_err(cx, format!("Keychain: {e}")),
                }
                this.refresh_keys(cx);
            })
            .ok();
        })
        .detach();
    }

    fn store_key(&mut self, provider: &'static str, cx: &mut Context<Self>) {
        let Some(input) = self.key_inputs.get(provider) else {
            return;
        };
        let key = input.read(cx).value().trim().to_string();
        if !key.is_empty() {
            self.key_op(provider, Some(key), cx);
        }
    }

    // ── Rendering ───────────────────────────────────────────────────

    fn input(&self, field: Field) -> Option<&Entity<InputState>> {
        self.inputs
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, i)| i)
    }

    fn slider(&self, fraction: Fraction) -> Option<&Entity<SliderState>> {
        self.sliders
            .iter()
            .find(|(f, _)| *f == fraction)
            .map(|(_, s)| s)
    }

    fn number(&self, field: Field) -> AnyElement {
        match self.input(field) {
            Some(i) => div().w(px(128.)).child(Input::new(i)).into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn wide(&self, field: Field) -> AnyElement {
        match self.input(field) {
            Some(i) => div()
                .w(px(360.))
                .font_family("monospace")
                .child(Input::new(i))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn percent(&self, fraction: Fraction, form: &Config) -> AnyElement {
        match self.slider(fraction) {
            Some(s) => h_flex()
                .w(px(220.))
                .gap_3()
                .child(div().flex_1().child(Slider::new(s)))
                .child(div().w(px(40.)).text_right().child(widgets::mono(
                    format!("{:.0}%", fraction.read(form) * 100.),
                    TEXT_MUTED,
                )))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn switch(
        &self,
        id: &'static str,
        on: bool,
        f: fn(&mut Config, bool),
        cx: &mut Context<Self>,
    ) -> Switch {
        Switch::new(id)
            .checked(on)
            .on_click(cx.listener(move |this, checked: &bool, _, cx| this.toggle(f, *checked, cx)))
    }

    fn section_body(&self, form: &Config, cx: &mut Context<Self>) -> Div {
        match self.section {
            Section::Inference => v_flex()
                .gap_4()
                .child(widgets::field("Default Context Length", Some("Tokens per conversation turn"), self.number(Field::ContextLength)))
                .child(widgets::field("Min Tokens/sec", Some("Models below this speed are excluded"), self.number(Field::MinTokensPerSec)))
                .child(widgets::field(
                    "Max Memory Usage",
                    Some("Fraction of available memory"),
                    self.percent(Fraction::MaxMemory, form),
                )),
            Section::Discovery => v_flex()
                .gap_4()
                .child(widgets::field("Top N Recommendations", None, self.number(Field::TopN)))
                .child(widgets::field("Models Directory", None, self.wide(Field::ModelsDir)))
                .child(widgets::field(
                    "Share Benchmarks",
                    Some("Anonymously contribute to community database"),
                    self.switch("share-benchmarks", form.share_benchmarks, |c, v| c.share_benchmarks = v, cx),
                ))
                .child(widgets::field(
                    "Crash Reports",
                    Some("Send anonymous crash reports so we can fix what breaks. No prompts, no chat history, no account details. Takes effect at the next launch."),
                    self.switch("crash-reports", form.telemetry.enabled, |c, v| c.telemetry.enabled = v, cx),
                ))
                .child(widgets::field(
                    "Usage Counts",
                    Some("Send anonymous usage counts (first launch, first chat, benchmark shared). No prompts, no content, no IP stored."),
                    self.switch("usage-counts", form.telemetry.usage_events, |c, v| c.telemetry.usage_events = v, cx),
                )),
            Section::Mesh => v_flex()
                .gap_4()
                .child(widgets::field(
                    "Enable Mesh",
                    Some("Join P2P network to share distributed compute"),
                    self.switch("mesh-enabled", form.mesh.enabled, |c, v| c.mesh.enabled = v, cx),
                ))
                .child(widgets::field("Port", Some("QUIC transport port (1024-65535)"), self.number(Field::MeshPort)))
                .child(widgets::field("Coordination Server", None, self.wide(Field::CoordinationServer)))
                .child(widgets::field("Max Contribution", Some("Resources to share"), self.percent(Fraction::MaxContribution, form)))
                .child(widgets::field(
                    "Verification Rate",
                    Some("Fraction of tokens to spot-check"),
                    self.percent(Fraction::VerificationRate, form),
                )),
            Section::Cloud => v_flex().gap_4().child(self.cloud_keys(cx)).child(widgets::field(
                "Cloud Fallback",
                Some("Fall back to cloud when local inference fails"),
                self.switch("cloud-fallback", form.cloud.cloud_fallback, |c, v| c.cloud.cloud_fallback = v, cx),
            )),
            Section::Storage => match &self.storage {
                None => v_flex().child(widgets::muted("Loading storage info...")),
                Some(report) => {
                    let n = report.models.len();
                    let summary = h_flex()
                        .gap_2()
                        .child(widgets::secondary("Total:"))
                        .child(div().text_sm().font_family("monospace").font_weight(FontWeight::MEDIUM).child(format_bytes(report.total_bytes)))
                        .child(widgets::badge(format!("{n} model{}", if n == 1 { "" } else { "s" }), Tone::Default))
                        .when(!report.partial_downloads.is_empty(), |d| {
                            d.child(widgets::badge(format!("{} partial", report.partial_downloads.len()), Tone::Warning))
                        })
                        .when(!report.orphaned_files.is_empty(), |d| {
                            d.child(widgets::badge(format!("{} orphaned", report.orphaned_files.len()), Tone::Danger))
                        });
                    v_flex().gap_2().child(summary).children(report.models.iter().map(|m| {
                        h_flex()
                            .justify_between()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(c(SURFACE_OVERLAY))
                            .child(div().text_xs().text_color(c(TEXT_SECONDARY)).truncate().child(m.model_name.clone()))
                            .child(widgets::mono(format_bytes(m.size_bytes), TEXT_MUTED))
                    }))
                }
            },
            Section::About => v_flex()
                .gap_3()
                .child(
                    h_flex()
                        .gap_3()
                        .child(Icon::new(IconName::Info).size(px(24.)).text_color(c(PAW_500)))
                        .child(
                            v_flex()
                                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("HiveBear Desktop"))
                                .child(widgets::mono(format!("v{}", env!("CARGO_PKG_VERSION")), TEXT_MUTED)),
                        ),
                )
                .child(v_flex().gap_1().children(ABOUT_LINKS.iter().map(|(label, url)| {
                    h_flex()
                        .id(*label)
                        .justify_between()
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .text_sm()
                        .cursor_pointer()
                        .text_color(c(TEXT_SECONDARY))
                        .hover(|s| s.bg(c(SURFACE_OVERLAY)).text_color(c(TEXT_PRIMARY)))
                        .child(*label)
                        .child(Icon::new(IconName::ExternalLink).size(px(12.)).text_color(c(TEXT_MUTED)))
                        .on_click(move |_, _, _| open_url(url))
                })))
                .child(widgets::muted(
                    "HiveBear is open-source software built by BeckhamLabs. Your device identity is a local Ed25519 keypair — it never leaves your machine.",
                )),
        }
    }

    /// Provider keys live in the OS keychain, not the config file, and are
    /// never read back for display — a row only shows whether one is stored.
    fn cloud_keys(&self, cx: &mut Context<Self>) -> Div {
        if !self.keys_checked {
            return v_flex().child(widgets::muted("Checking the keychain…"));
        }
        if !self.keychain_available {
            return v_flex().child(widgets::banner(
                WARNING,
                "No OS keychain is available, so cloud API keys can't be stored securely here.",
            ));
        }
        let is_set = |p: &str| self.configured_keys.iter().any(|k| k == p);
        let mut col = v_flex().gap_1().child(
            h_flex()
                .gap_1()
                .text_xs()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(PAW_400))
                        .child(self.configured_keys.len().to_string()),
                )
                .child(
                    div()
                        .text_color(c(TEXT_MUTED))
                        .child(format!("of {} providers configured", CLOUD_PROVIDERS.len())),
                ),
        );
        for p in CLOUD_PROVIDERS.iter().filter(|p| is_set(p.0)) {
            col = col.child(self.provider_row(p, true, cx));
        }
        let popular: Vec<_> = CLOUD_PROVIDERS
            .iter()
            .filter(|p| POPULAR_PROVIDERS.contains(&p.0) && !is_set(p.0))
            .collect();
        if !popular.is_empty() {
            col = col.child(div().pt_2().child(widgets::overline("Add Provider")));
            for p in popular {
                col = col.child(self.provider_row(p, false, cx));
            }
        }
        let rest: Vec<_> = CLOUD_PROVIDERS
            .iter()
            .filter(|p| !POPULAR_PROVIDERS.contains(&p.0) && !is_set(p.0))
            .collect();
        if !rest.is_empty() {
            let label = format!(
                "{} {} more providers",
                if self.show_all_providers {
                    "Hide"
                } else {
                    "Show"
                },
                rest.len()
            );
            col = col.child(
                widgets::button("toggle-providers", label)
                    .icon(if self.show_all_providers {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_all_providers = !this.show_all_providers;
                        cx.notify();
                    })),
            );
            if self.show_all_providers {
                for p in rest {
                    col = col.child(self.provider_row(p, false, cx));
                }
            }
        }
        col
    }

    fn provider_row(
        &self,
        p: &'static (&'static str, &'static str, &'static str),
        is_set: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let (prefix, name, _) = *p;
        let Some(input) = self.key_inputs.get(prefix) else {
            return div();
        };
        let has_input = !input.read(cx).value().trim().is_empty();
        let visible = self.key_visible.contains(&prefix);
        h_flex()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(8.))
            .bg(c(SURFACE_OVERLAY))
            .child(
                Icon::new(if is_set {
                    IconName::Check
                } else {
                    IconName::Cloud
                })
                .size(px(12.))
                .text_color(c(if is_set { SUCCESS } else { TEXT_MUTED })),
            )
            .child(
                div()
                    .w(px(140.))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(name),
            )
            .child(div().flex_1().child(Input::new(input).small()))
            .when(has_input, |d| {
                let input = input.clone();
                d.child(
                    widgets::icon_button(
                        SharedString::from(format!("eye-{prefix}")),
                        if visible {
                            IconName::EyeOff
                        } else {
                            IconName::Eye
                        },
                        "Show key",
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let show = !this.key_visible.contains(&prefix);
                        if show {
                            this.key_visible.push(prefix);
                        } else {
                            this.key_visible.retain(|p| *p != prefix);
                        }
                        input.update(cx, |i, cx| i.set_masked(!show, window, cx));
                        cx.notify();
                    })),
                )
                .child(
                    widgets::button(SharedString::from(format!("store-{prefix}")), "Save")
                        .primary()
                        .xsmall()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.store_key(prefix, cx);
                            if let Some(input) = this.key_inputs.get(prefix) {
                                input.update(cx, |i, cx| i.set_value("", window, cx));
                            }
                        })),
                )
            })
            .when(is_set, |d| {
                d.child(
                    widgets::button(SharedString::from(format!("remove-key-{prefix}")), "Remove")
                        .icon(IconName::Trash)
                        .danger()
                        .xsmall()
                        .tooltip("Remove from keychain")
                        .on_click(cx.listener(move |this, _, _, cx| this.key_op(prefix, None, cx))),
                )
            })
            .child(
                div()
                    .w(px(72.))
                    .text_right()
                    .child(widgets::mono(format!("{prefix}/"), TEXT_MUTED)),
            )
    }
}

impl Render for Settings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(form) = self.form.clone() else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(widgets::muted("Loading configuration..."))
                .into_any_element();
        };

        let mut nav = v_flex().w(px(128.)).pt_12().gap_0p5();
        for s in SECTIONS {
            let on = s == self.section;
            nav = nav.child(
                div()
                    .id(s.label())
                    .px_3()
                    .py_1p5()
                    .rounded(px(8.))
                    .text_xs()
                    .cursor_pointer()
                    .text_color(c(if on { TEXT_PRIMARY } else { TEXT_MUTED }))
                    .when(on, |d| {
                        d.bg(c(SURFACE_OVERLAY)).font_weight(FontWeight::MEDIUM)
                    })
                    .hover(|st| st.text_color(c(TEXT_SECONDARY)))
                    .child(s.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = s;
                        cx.notify();
                    })),
            );
        }

        let dirty = self.is_dirty();
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .gap_6()
            .child(widgets::page_title("Settings"))
            .children(self.error.clone().map(|e| widgets::banner(DANGER, e)))
            .child(
                widgets::card().child(
                    v_flex()
                        .gap_4()
                        .child(widgets::section_title(self.section.label()))
                        .child(self.section_body(&form, cx)),
                ),
            )
            .when(dirty, |d| {
                d.child(
                    widgets::raised_card().child(
                        h_flex()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .text_color(c(TEXT_SECONDARY))
                                    .child("Unsaved changes"),
                            )
                            .child(
                                widgets::button("discard", "Discard")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(saved) = this.saved.clone() {
                                            this.load_from(saved, window, cx);
                                        }
                                    })),
                            )
                            .child(
                                widgets::button("save", "Save")
                                    .icon(IconName::Save)
                                    .primary()
                                    .small()
                                    .loading(self.saving)
                                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                            ),
                    ),
                )
            });

        widgets::scroll_page(
            "settings",
            h_flex().items_start().gap_8().child(nav).child(content),
        )
        .into_any_element()
    }
}
