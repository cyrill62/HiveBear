use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::backend::account::{self as acct, AccountInfo, ApiKeyInfo, UsageSummary};
use crate::error::CmdResult;
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::util::open_url;
use crate::ui::widgets;
use crate::ui::{cmd, copy, rt, toast_err, Navigate};

pub struct Account {
    state: Arc<AppState>,
    account: Option<AccountInfo>,
    loaded: bool,
    submitting: bool,
    error: Option<String>,

    show_email: bool,
    register: bool,
    email: Entity<InputState>,
    password: Entity<InputState>,
    display_name: Entity<InputState>,
    show_password: bool,

    usage: Option<UsageSummary>,
    usage_loading: bool,
    keys: Vec<ApiKeyInfo>,
    keys_loading: bool,
    key_label: Entity<InputState>,
    /// A freshly created key, shown once so it can be copied.
    new_key: Option<String>,
    creating_key: bool,
    checking_out: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Navigate> for Account {}

impl Account {
    pub fn new(
        state: Arc<AppState>,
        _: Entity<Shared>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("Email"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password (min 8 characters)")
                .masked(true)
        });
        let display_name = cx.new(|cx| InputState::new(window, cx).placeholder("Display name"));
        let key_label = cx.new(|cx| InputState::new(window, cx).placeholder("Label"));
        let submit_on_enter =
            |this: &mut Self, _: Entity<InputState>, event: &InputEvent, cx: &mut Context<Self>| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(cx);
                }
            };
        let _subscriptions = vec![
            cx.subscribe(&email, submit_on_enter),
            cx.subscribe(&password, submit_on_enter),
        ];
        Self {
            state,
            account: None,
            loaded: false,
            submitting: false,
            error: None,
            show_email: false,
            register: false,
            email,
            password,
            display_name,
            show_password: false,
            usage: None,
            usage_loading: false,
            keys: vec![],
            keys_loading: false,
            key_label,
            new_key: None,
            creating_key: false,
            checking_out: false,
            _subscriptions,
        }
    }

    pub fn enter(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move { acct::get_account(&s).await }).await;
            this.update(cx, |this, cx| {
                this.loaded = true;
                // Offline or not authenticated — both are fine.
                this.account = result.ok().flatten();
                if this.account.is_some() {
                    this.refresh_details(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_details(&mut self, cx: &mut Context<Self>) {
        self.usage_loading = true;
        self.keys_loading = true;
        let (s1, s2) = (Arc::clone(&self.state), Arc::clone(&self.state));
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let usage = cmd(
                rt.clone(),
                async move { acct::get_usage_summary(&s1).await },
            )
            .await;
            let keys = cmd(rt, async move { acct::list_api_keys(&s2).await }).await;
            this.update(cx, |this, cx| {
                this.usage_loading = false;
                this.keys_loading = false;
                // Offline — the section says so.
                this.usage = usage.ok();
                if let Ok(keys) = keys {
                    this.keys = keys;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Run an auth call, then reload the account on success.
    fn authenticate<F>(&mut self, cx: &mut Context<Self>, call: impl FnOnce(Arc<AppState>) -> F)
    where
        F: std::future::Future<Output = CmdResult<acct::AuthResult>> + Send + 'static,
    {
        if self.submitting {
            return;
        }
        self.error = None;
        self.submitting = true;
        cx.notify();
        let fut = call(Arc::clone(&self.state));
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, fut).await;
            this.update(cx, |this, cx| {
                this.submitting = false;
                match result {
                    Ok(_) => this.refresh(cx),
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let email = self.email.read(cx).value().trim().to_string();
        let password = self.password.read(cx).value().to_string();
        let name = self.display_name.read(cx).value().trim().to_string();
        let problem = if !email.contains('@') {
            Some("Enter a valid email address.")
        } else if password.chars().count() < 8 {
            Some("Password must be at least 8 characters.")
        } else if self.register && name.is_empty() {
            Some("Enter a display name.")
        } else {
            None
        };
        if let Some(p) = problem {
            self.error = Some(p.into());
            cx.notify();
            return;
        }
        if self.register {
            self.authenticate(cx, move |s| async move {
                acct::register(&s, &email, &password, &name).await
            });
        } else {
            self.authenticate(cx, move |s| async move {
                acct::login(&s, &email, &password).await
            });
        }
    }

    fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Clear the typed password rather than keeping it around for the next form.
        self.password
            .update(cx, |p, cx| p.set_value("", window, cx));
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let _ = cmd(rt, async move { acct::logout(&s).await }).await;
            this.update(cx, |this, cx| {
                this.account = None;
                this.usage = None;
                this.keys.clear();
                this.new_key = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn create_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.creating_key {
            return;
        }
        self.creating_key = true;
        cx.notify();
        let label = match self.key_label.read(cx).value().trim() {
            "" => "default".to_string(),
            l => l.to_string(),
        };
        self.key_label
            .update(cx, |k, cx| k.set_value("", window, cx));
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move { acct::create_api_key(&s, &label).await }).await;
            this.update(cx, |this, cx| {
                this.creating_key = false;
                match result {
                    Ok(key) => {
                        this.new_key = Some(key.key);
                        this.refresh_details(cx);
                    }
                    Err(e) => toast_err(cx, e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn revoke_key(&mut self, id: String, cx: &mut Context<Self>) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move { acct::revoke_api_key(&s, &id).await }).await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.refresh_details(cx),
                Err(e) => toast_err(cx, e),
            })
            .ok();
        })
        .detach();
    }

    fn upgrade(&mut self, plan: &'static str, cx: &mut Context<Self>) {
        if self.checking_out {
            return;
        }
        self.checking_out = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move { acct::create_checkout(&s, plan).await }).await;
            this.update(cx, |this, cx| {
                this.checking_out = false;
                match result {
                    Ok(url) => open_url(&url),
                    Err(e) => toast_err(cx, e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn auth_choice(&self, cx: &mut Context<Self>) -> Div {
        let choice = |id: &'static str, icon: IconName, title: &'static str, desc: &'static str| {
            widgets::clickable_card(id).flex_1().p_6().child(
                v_flex()
                    .items_center()
                    .gap_3()
                    .child(Icon::new(icon).size(px(32.)).text_color(c(PAW_500)))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                    .child(
                        div()
                            .text_xs()
                            .text_center()
                            .text_color(c(TEXT_SECONDARY))
                            .child(desc),
                    ),
            )
        };
        v_flex()
            .w(px(672.))
            .gap_6()
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Choose how to sign in"),
                    )
                    .child(widgets::secondary(
                        "Both paths give you the same features. Pick what suits you.",
                    )),
            )
            .children(self.error.clone().map(|e| widgets::banner(DANGER, e)))
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        choice(
                            "choose-email",
                            IconName::Mail,
                            "Email Account",
                            "For teams & billing management. Password recovery included.",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_email = true;
                            this.error = None;
                            cx.notify();
                        })),
                    )
                    .child(
                        choice(
                            "choose-device",
                            IconName::FingerprintPattern,
                            "Device Key",
                            "Maximum privacy. No email required. Mullvad-style anonymous.",
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.authenticate(
                                cx,
                                |s| async move { acct::activate_device(&s).await },
                            )
                        })),
                    ),
            )
            .when(self.submitting, |d| {
                d.child(div().text_center().child(widgets::muted("Signing in…")))
            })
    }

    fn email_form(&self, cx: &mut Context<Self>) -> Div {
        let tab =
            |id: &'static str, label: &'static str, register: bool, cx: &mut Context<Self>| {
                let on = self.register == register;
                div()
                    .id(id)
                    .flex_1()
                    .py_1p5()
                    .rounded(px(6.))
                    .text_center()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .text_color(c(if on { TEXT_PRIMARY } else { TEXT_SECONDARY }))
                    .when(on, |d| d.bg(c(SURFACE_OVERLAY)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.register = register;
                        cx.notify();
                    }))
            };
        let label = if self.register {
            "Create Account"
        } else {
            "Sign In"
        };
        v_flex()
            .w(px(384.))
            .gap_3()
            .child(
                div()
                    .id("email-back")
                    .text_sm()
                    .cursor_pointer()
                    .text_color(c(TEXT_SECONDARY))
                    .hover(|s| s.text_color(c(TEXT_PRIMARY)))
                    .child("← Back")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_email = false;
                        this.error = None;
                        cx.notify();
                    })),
            )
            .child(
                h_flex()
                    .p_0p5()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(c(BORDER))
                    .child(tab("tab-login", "Sign In", false, cx))
                    .child(tab("tab-register", "Register", true, cx)),
            )
            .children(self.error.clone().map(|e| widgets::banner(DANGER, e)))
            .when(self.register, |d| d.child(Input::new(&self.display_name)))
            .child(Input::new(&self.email))
            .child(
                Input::new(&self.password).suffix(
                    widgets::icon_button(
                        "toggle-password",
                        if self.show_password {
                            IconName::EyeOff
                        } else {
                            IconName::Eye
                        },
                        "Show password",
                    )
                    .xsmall()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_password = !this.show_password;
                        let masked = !this.show_password;
                        this.password
                            .update(cx, |p, cx| p.set_masked(masked, window, cx));
                        cx.notify();
                    })),
                ),
            )
            .child(
                widgets::button("submit-auth", label)
                    .primary()
                    .w_full()
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
            )
    }

    fn dashboard(&self, account: &AccountInfo, cx: &mut Context<Self>) -> Div {
        let device = account.auth_mode == "device";
        let identity = if device {
            let key: String = account
                .pubkey
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(16)
                .collect();
            v_flex()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child("Anonymous Device"),
                )
                .child(widgets::mono(format!("{key}..."), TEXT_SECONDARY))
        } else {
            v_flex()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(account.display_name.clone().unwrap_or_default()),
                )
                .child(widgets::secondary(
                    account.email.clone().unwrap_or_default(),
                ))
        };
        let identity_card = widgets::card().p_5().child(
            h_flex()
                .gap_3()
                .child(
                    Icon::new(if device {
                        IconName::FingerprintPattern
                    } else {
                        IconName::CircleUser
                    })
                    .size(px(24.))
                    .text_color(c(PAW_500)),
                )
                .child(div().flex_1().child(identity))
                .child(tier_badge(&account.tier))
                .child(
                    widgets::button("sign-out", "Sign out")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| this.logout(window, cx))),
                ),
        );

        let subscription = if account.tier == "community" {
            v_flex()
                .gap_3()
                .child(div().text_sm().child(
                    "You're on the Community plan. Upgrade for priority mesh, private hives, and unlimited cloud.",
                ))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            widgets::button("upgrade-pro", "Upgrade to Pro — $15/mo")
                                .primary()
                                .loading(self.checking_out)
                                .on_click(cx.listener(|this, _, _, cx| this.upgrade("pro", cx))),
                        )
                        .when(account.auth_mode == "email", |d| {
                            d.child(
                                widgets::button("upgrade-team", "Team — $10/seat/mo")
                                    .outline()
                                    .on_click(cx.listener(|this, _, _, cx| this.upgrade("team", cx))),
                            )
                        }),
                )
        } else {
            let renews = account
                .current_period_end
                .as_ref()
                .map(|end| format!("· renews {}", end.split('T').next().unwrap_or(end)));
            v_flex().child(
                h_flex()
                    .gap_2()
                    .child(tier_badge(&account.tier))
                    .child(div().text_sm().child("plan active"))
                    .children(renews.map(widgets::secondary)),
            )
        };

        let usage = if self.usage_loading {
            widgets::muted("Loading...")
        } else if let Some(u) = &self.usage {
            let limit = u
                .cloud_requests_limit
                .map(|l| format!(" / {l}"))
                .unwrap_or_default();
            h_flex()
                .child(big_number(
                    u.cloud_requests_today.to_string(),
                    format!("Cloud requests today{limit}"),
                ))
                .child(big_number(
                    format!("{}m", (u.mesh_seconds_contributed as f64 / 60.0).round()),
                    "Mesh contributed".into(),
                ))
                .child(big_number(
                    u.model_downloads_this_month.to_string(),
                    "Downloads this month".into(),
                ))
        } else {
            widgets::muted("Offline — usage data unavailable")
        };

        let mut keys = v_flex().gap_2().child(
            h_flex()
                .justify_between()
                .child(section_label("API Keys"))
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().w(px(140.)).child(Input::new(&self.key_label).small()))
                        .child(
                            widgets::button("create-key", "Create")
                                .icon(IconName::Plus)
                                .primary()
                                .small()
                                .loading(self.creating_key)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.create_key(window, cx)),
                                ),
                        ),
                ),
        );
        if let Some(key) = &self.new_key {
            keys = keys.child(
                div()
                    .p_3()
                    .rounded(px(8.))
                    .bg(ca(SUCCESS, 0.1))
                    .border_1()
                    .border_color(ca(SUCCESS, 0.3))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(SUCCESS))
                            .child("New API key created — copy it now, it won't be shown again:"),
                    )
                    .child(
                        h_flex()
                            .pt_1()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(c(SURFACE))
                                    .child(widgets::mono(key.clone(), TEXT_PRIMARY)),
                            )
                            .child(
                                widgets::icon_button("copy-key", IconName::Copy, "Copy").on_click(
                                    {
                                        let key = key.clone();
                                        move |_, _, cx| copy(cx, key.clone())
                                    },
                                ),
                            ),
                    ),
            );
        }
        if self.keys_loading && self.keys.is_empty() {
            keys = keys.child(widgets::muted("Loading..."));
        } else if self.keys.is_empty() {
            keys = keys.child(widgets::muted("No API keys yet."));
        }
        for key in &self.keys {
            let id = key.id.clone();
            keys = keys.child(
                widgets::inset().child(
                    h_flex()
                        .gap_2()
                        .child(widgets::mono(
                            format!("{}...", key.key_prefix),
                            TEXT_PRIMARY,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(c(TEXT_SECONDARY))
                                .child(key.label.clone()),
                        )
                        .child(
                            widgets::icon_button(
                                SharedString::from(format!("revoke-{}", key.id)),
                                IconName::Trash,
                                "Revoke",
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.revoke_key(id.clone(), cx)),
                            ),
                        ),
                ),
            );
        }

        v_flex()
            .gap_4()
            .child(identity_card)
            .child(
                widgets::card().p_5().child(
                    v_flex()
                        .gap_3()
                        .child(section_label("Subscription"))
                        .child(subscription),
                ),
            )
            .child(
                widgets::card()
                    .p_5()
                    .child(v_flex().gap_3().child(section_label("Usage")).child(usage)),
            )
            .child(widgets::card().p_5().child(keys))
    }
}

impl Render for Account {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.loaded {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(widgets::muted("Loading account..."))
                .into_any_element();
        }
        if let Some(account) = self.account.clone() {
            return widgets::scroll_page(
                "account",
                h_flex().justify_center().child(
                    v_flex()
                        .w_full()
                        .max_w(px(672.))
                        .gap_4()
                        .child(widgets::page_title("Account"))
                        .child(self.dashboard(&account, cx)),
                ),
            )
            .into_any_element();
        }
        let content = if self.show_email {
            self.email_form(cx)
        } else {
            self.auth_choice(cx)
        };
        widgets::scroll_page("account", h_flex().pt_12().justify_center().child(content))
            .into_any_element()
    }
}

fn section_label(text: &'static str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(TEXT_SECONDARY))
        .child(text)
}

fn tier_badge(tier: &str) -> Div {
    let (color, icon): (u32, Option<IconName>) = match tier {
        "pro" => (0xfcd34d, Some(IconName::Shield)),
        "team" => (0x93c5fd, Some(IconName::KeyRound)),
        "enterprise" => (0xd8b4fe, Some(IconName::Crown)),
        _ => (0xd4d4d8, None),
    };
    let name: String = tier
        .chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .chain(tier.chars().skip(1))
        .collect();
    h_flex()
        .gap_1()
        .px_2p5()
        .py_0p5()
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(color))
        .bg(ca(color, 0.12))
        .border_1()
        .border_color(ca(color, 0.4))
        .children(icon.map(|i| Icon::new(i).size(px(12.))))
        .child(name)
}

fn big_number(value: String, caption: String) -> Div {
    v_flex()
        .flex_1()
        .child(div().text_2xl().font_weight(FontWeight::BOLD).child(value))
        .child(div().text_xs().text_color(c(TEXT_SECONDARY)).child(caption))
}
