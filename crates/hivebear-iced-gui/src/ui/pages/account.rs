use iced::widget::{column, container, row, space, text_input};
use iced::{Alignment, Color, Element, Fill, Task};

use crate::backend::account::{
    self as acct, AccountInfo, ApiKeyInfo, AuthResult, NewApiKey, UsageSummary,
};
use crate::error::CmdResult;
use crate::ui::icon::{self, icon};
use crate::ui::theme::{self, *};
use crate::ui::widgets::{self, Kind, Pad};
use crate::ui::{call, scroll_page, Cx, Env};

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(CmdResult<Option<AccountInfo>>),
    ShowEmail(bool),
    SetRegister(bool),
    Email(String),
    Password(String),
    DisplayName(String),
    TogglePassword,
    Submit,
    ActivateDevice,
    Authed(CmdResult<AuthResult>),
    Logout,
    LoggedOut,
    Usage(CmdResult<UsageSummary>),
    Keys(CmdResult<Vec<ApiKeyInfo>>),
    KeyLabel(String),
    CreateKey,
    KeyCreated(CmdResult<NewApiKey>),
    CopyKey,
    RevokeKey(String),
    Revoked(CmdResult<()>),
    Upgrade(&'static str),
    Checkout(CmdResult<String>),
}

#[derive(Default)]
pub struct Account {
    account: Option<AccountInfo>,
    loaded: bool,
    submitting: bool,
    error: Option<String>,

    show_email: bool,
    register: bool,
    email: String,
    password: String,
    display_name: String,
    show_password: bool,

    usage: Option<UsageSummary>,
    usage_loading: bool,
    keys: Vec<ApiKeyInfo>,
    keys_loading: bool,
    new_key_label: String,
    /// A freshly created key, shown once so it can be copied.
    new_key: Option<String>,
    creating_key: bool,
    checking_out: bool,
}

impl Account {
    pub fn enter(&mut self, env: &mut Env) -> Task<Msg> {
        call(
            env.state,
            |s| async move { acct::get_account(&s).await },
            Msg::Loaded,
        )
    }

    fn refresh_details(&mut self, env: &Env) -> Task<Msg> {
        self.usage_loading = true;
        self.keys_loading = true;
        Task::batch([
            call(
                env.state,
                |s| async move { acct::get_usage_summary(&s).await },
                Msg::Usage,
            ),
            call(
                env.state,
                |s| async move { acct::list_api_keys(&s).await },
                Msg::Keys,
            ),
        ])
    }

    fn submit(&mut self, env: &Env) -> Task<Msg> {
        if self.submitting {
            return Task::none();
        }
        let email = self.email.trim().to_string();
        let name = self.display_name.trim().to_string();
        let problem = if !email.contains('@') {
            Some("Enter a valid email address.")
        } else if self.password.chars().count() < 8 {
            Some("Password must be at least 8 characters.")
        } else if self.register && name.is_empty() {
            Some("Enter a display name.")
        } else {
            None
        };
        if let Some(p) = problem {
            self.error = Some(p.into());
            return Task::none();
        }
        self.error = None;
        self.submitting = true;
        let password = self.password.clone();
        if self.register {
            call(
                env.state,
                |s| async move { acct::register(&s, &email, &password, &name).await },
                Msg::Authed,
            )
        } else {
            call(
                env.state,
                |s| async move { acct::login(&s, &email, &password).await },
                Msg::Authed,
            )
        }
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            Msg::Loaded(result) => {
                self.loaded = true;
                // Offline or not authenticated — both are fine.
                self.account = result.ok().flatten();
                if self.account.is_some() {
                    return self.refresh_details(env);
                }
            }
            Msg::ShowEmail(show) => {
                self.show_email = show;
                self.error = None;
            }
            Msg::SetRegister(r) => self.register = r,
            Msg::Email(v) => self.email = v,
            Msg::Password(v) => self.password = v,
            Msg::DisplayName(v) => self.display_name = v,
            Msg::TogglePassword => self.show_password = !self.show_password,
            Msg::Submit => return self.submit(env),
            Msg::ActivateDevice if !self.submitting => {
                self.error = None;
                self.submitting = true;
                return call(
                    env.state,
                    |s| async move { acct::activate_device(&s).await },
                    Msg::Authed,
                );
            }
            Msg::ActivateDevice => {}
            Msg::Authed(result) => {
                self.submitting = false;
                match result {
                    Ok(_) => {
                        self.password.clear();
                        return self.enter(env);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Msg::Logout => {
                return call(
                    env.state,
                    |s| async move { acct::logout(&s).await },
                    |_| Msg::LoggedOut,
                );
            }
            Msg::LoggedOut => {
                self.account = None;
                self.usage = None;
                self.keys.clear();
                self.new_key = None;
            }
            Msg::Usage(result) => {
                self.usage_loading = false;
                // Offline — the section says so.
                self.usage = result.ok();
            }
            Msg::Keys(result) => {
                self.keys_loading = false;
                if let Ok(keys) = result {
                    self.keys = keys;
                }
            }
            Msg::KeyLabel(v) => self.new_key_label = v,
            Msg::CreateKey if !self.creating_key => {
                self.creating_key = true;
                let label = match self.new_key_label.trim() {
                    "" => "default".to_string(),
                    l => l.to_string(),
                };
                return call(
                    env.state,
                    |s| async move { acct::create_api_key(&s, &label).await },
                    Msg::KeyCreated,
                );
            }
            Msg::CreateKey => {}
            Msg::KeyCreated(result) => {
                self.creating_key = false;
                match result {
                    Ok(key) => {
                        self.new_key = Some(key.key);
                        self.new_key_label.clear();
                        return self.refresh_details(env);
                    }
                    Err(e) => env.toasts.error(e),
                }
            }
            Msg::CopyKey => {
                if let Some(key) = &self.new_key {
                    env.copy(key.clone());
                }
            }
            Msg::RevokeKey(id) => {
                return call(
                    env.state,
                    |s| async move { acct::revoke_api_key(&s, &id).await },
                    Msg::Revoked,
                );
            }
            Msg::Revoked(result) => match result {
                Ok(()) => return self.refresh_details(env),
                Err(e) => env.toasts.error(e),
            },
            Msg::Upgrade(plan) if !self.checking_out => {
                self.checking_out = true;
                return call(
                    env.state,
                    |s| async move { acct::create_checkout(&s, plan).await },
                    Msg::Checkout,
                );
            }
            Msg::Upgrade(_) => {}
            Msg::Checkout(result) => {
                self.checking_out = false;
                match result {
                    Ok(url) => crate::ui::util::open_url(&url),
                    Err(e) => env.toasts.error(e),
                }
            }
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, _cx: &Cx<'a>) -> Element<'a, Msg> {
        if !self.loaded {
            return container(widgets::muted("Loading account..."))
                .center(Fill)
                .into();
        }
        if let Some(account) = &self.account {
            return scroll_page(
                container(
                    column![widgets::page_title("Account"), self.dashboard(account)].spacing(16),
                )
                .max_width(672)
                .center_x(Fill),
            );
        }
        let content = if self.show_email {
            self.email_form()
        } else {
            self.auth_choice()
        };
        scroll_page(container(content).padding([48, 0]).center_x(Fill))
    }

    fn auth_choice(&self) -> Element<'_, Msg> {
        let mut col = column![
            widgets::label("Choose how to sign in", 20.0, TEXT_PRIMARY).font(widgets::BOLD),
            widgets::secondary("Both paths give you the same features. Pick what suits you."),
        ]
        .spacing(8)
        .align_x(Alignment::Center)
        .max_width(672);
        if let Some(e) = &self.error {
            col = col.push(widgets::error_banner(e.as_str()));
        }
        let choice = |glyph: &'static str, title: &'static str, desc: &'static str, msg: Msg| {
            widgets::clickable_card(
                Pad::Lg,
                column![
                    icon(glyph).size(32).color(PAW_500),
                    widgets::strong(title),
                    widgets::label(desc, SMALL, TEXT_SECONDARY).center(),
                ]
                .spacing(8)
                .align_x(Alignment::Center)
                .width(Fill),
                msg,
            )
            .width(Fill)
        };
        let mut device = choice(
            icon::FINGERPRINT,
            "Device Key",
            "Maximum privacy. No email required. Mullvad-style anonymous.",
            Msg::ActivateDevice,
        );
        if self.submitting {
            device = device.on_press_maybe(None);
        }
        col = col.push(
            row![
                choice(
                    icon::ENVELOPE,
                    "Email Account",
                    "For teams & billing management. Password recovery included.",
                    Msg::ShowEmail(true)
                ),
                device,
            ]
            .spacing(16),
        );
        if self.submitting {
            col = col.push(widgets::muted("Signing in…"));
        }
        col.into()
    }

    fn email_form(&self) -> Element<'_, Msg> {
        let mut col = column![
            widgets::link(
                format!("{}  Back", icon::ARROW_LEFT),
                TEXT_SECONDARY,
                Msg::ShowEmail(false)
            ),
            widgets::segmented(
                vec!["Sign In".into(), "Register".into()],
                usize::from(self.register),
                |i| Msg::SetRegister(i == 1)
            ),
        ]
        .spacing(10)
        .max_width(384);
        if let Some(e) = &self.error {
            col = col.push(widgets::error_banner(e.as_str()));
        }
        if self.register {
            col = col.push(widgets::input(
                "Display name",
                &self.display_name,
                Msg::DisplayName,
            ));
        }
        col = col.push(widgets::input("Email", &self.email, Msg::Email).on_submit(Msg::Submit));
        col = col.push(
            row![
                widgets::input("Password (min 8 characters)", &self.password, Msg::Password)
                    .secure(!self.show_password)
                    .on_submit(Msg::Submit),
                widgets::icon_button(
                    if self.show_password {
                        icon::EYE_SLASH
                    } else {
                        icon::EYE
                    },
                    "Show password",
                    Msg::TogglePassword
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        );
        let label = if self.submitting {
            "…"
        } else if self.register {
            "Create Account"
        } else {
            "Sign In"
        };
        col = col.push(
            widgets::btn(
                Kind::Primary,
                None,
                label,
                (!self.submitting).then_some(Msg::Submit),
            )
            .width(Fill),
        );
        col.into()
    }

    fn dashboard<'a>(&'a self, account: &'a AccountInfo) -> Element<'a, Msg> {
        let device = account.auth_mode == "device";
        let identity: Element<'a, Msg> = if device {
            let key: String = account
                .pubkey
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(16)
                .collect();
            column![
                widgets::strong("Anonymous Device"),
                widgets::mono(format!("{key}..."), SMALL, TEXT_SECONDARY)
            ]
            .into()
        } else {
            column![
                widgets::strong(account.display_name.as_deref().unwrap_or_default()),
                widgets::secondary(account.email.as_deref().unwrap_or_default()),
            ]
            .into()
        };
        let identity_card = widgets::card(
            Pad::Lg,
            row![
                icon(if device {
                    icon::FINGERPRINT
                } else {
                    icon::USER_CIRCLE
                })
                .size(24)
                .color(PAW_500),
                identity,
                space::horizontal(),
                tier_badge(&account.tier),
                widgets::link("Sign out", TEXT_SECONDARY, Msg::Logout),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        );

        let subscription: Element<'a, Msg> = if account.tier == "community" {
            let upgrade = |label: &'static str, kind: Kind, plan: &'static str| {
                widgets::btn(
                    kind,
                    None,
                    label,
                    (!self.checking_out).then_some(Msg::Upgrade(plan)),
                )
            };
            let mut buttons =
                row![upgrade("Upgrade to Pro — $15/mo", Kind::Primary, "pro")].spacing(8);
            if account.auth_mode == "email" {
                buttons = buttons.push(upgrade("Team — $10/seat/mo", Kind::Secondary, "team"));
            }
            column![
                widgets::primary(
                    "You're on the Community plan. Upgrade for priority mesh, private hives, and unlimited cloud."
                ),
                buttons,
            ]
            .spacing(10)
            .into()
        } else {
            let mut line = row![tier_badge(&account.tier), widgets::primary("plan active")]
                .spacing(8)
                .align_y(Alignment::Center);
            if let Some(end) = &account.current_period_end {
                let date = end.split('T').next().unwrap_or(end);
                line = line.push(widgets::secondary(format!("· renews {date}")));
            }
            line.into()
        };

        let usage: Element<'a, Msg> = if self.usage_loading {
            widgets::muted("Loading...").into()
        } else if let Some(u) = &self.usage {
            let limit = u
                .cloud_requests_limit
                .map(|l| format!(" / {l}"))
                .unwrap_or_default();
            row![
                big_number(
                    u.cloud_requests_today.to_string(),
                    format!("Cloud requests today{limit}")
                ),
                big_number(
                    format!("{}m", (u.mesh_seconds_contributed as f64 / 60.0).round()),
                    "Mesh contributed".into()
                ),
                big_number(
                    u.model_downloads_this_month.to_string(),
                    "Downloads this month".into()
                ),
            ]
            .into()
        } else {
            widgets::muted("Offline — usage data unavailable").into()
        };

        let mut keys = column![row![
            widgets::label("API Keys", BODY, TEXT_SECONDARY).font(widgets::BOLD),
            space::horizontal(),
            widgets::input("Label", &self.new_key_label, Msg::KeyLabel).width(140),
            widgets::busy_btn(
                Kind::Primary,
                icon::PLUS,
                "Create",
                "Creating…",
                self.creating_key,
                Some(Msg::CreateKey)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)]
        .spacing(8);
        if let Some(key) = &self.new_key {
            keys = keys.push(
                container(
                    column![
                        widgets::label(
                            "New API key created — copy it now, it won't be shown again:",
                            SMALL,
                            SUCCESS
                        ),
                        row![
                            text_input("", key)
                                .font(widgets::MONO)
                                .size(SMALL)
                                .style(theme::input),
                            widgets::icon_button(icon::COPY, "Copy", Msg::CopyKey),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(6),
                )
                .padding(10)
                .style(theme::banner(SUCCESS)),
            );
        }
        if self.keys_loading && self.keys.is_empty() {
            keys = keys.push(widgets::muted("Loading..."));
        } else if self.keys.is_empty() {
            keys = keys.push(widgets::muted("No API keys yet."));
        }
        for key in &self.keys {
            keys = keys.push(
                container(
                    row![
                        widgets::mono(format!("{}...", key.key_prefix), SMALL, TEXT_PRIMARY),
                        widgets::label(key.label.as_str(), SMALL, TEXT_SECONDARY),
                        space::horizontal(),
                        widgets::icon_button(icon::TRASH, "Revoke", Msg::RevokeKey(key.id.clone())),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([4, 12])
                .style(theme::inset),
            );
        }

        column![
            identity_card,
            widgets::card(
                Pad::Lg,
                column![
                    widgets::label("Subscription", BODY, TEXT_SECONDARY).font(widgets::BOLD),
                    subscription
                ]
                .spacing(10)
            ),
            widgets::card(
                Pad::Lg,
                column![
                    widgets::label("Usage", BODY, TEXT_SECONDARY).font(widgets::BOLD),
                    usage
                ]
                .spacing(10)
            ),
            widgets::card(Pad::Lg, keys),
        ]
        .spacing(16)
        .into()
    }
}

fn tier_badge<'a>(tier: &str) -> Element<'a, Msg> {
    let (color, glyph) = match tier {
        "pro" => (Color::from_rgb8(0xfc, 0xd3, 0x4d), Some(icon::SHIELD)),
        "team" => (Color::from_rgb8(0x93, 0xc5, 0xfd), Some(icon::KEY)),
        "enterprise" => (Color::from_rgb8(0xd8, 0xb4, 0xfe), Some(icon::CROWN)),
        _ => (Color::from_rgb8(0xd4, 0xd4, 0xd8), None),
    };
    let name: String = tier
        .chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .chain(tier.chars().skip(1))
        .collect();
    let mut content = row![].spacing(4).align_y(Alignment::Center);
    if let Some(g) = glyph {
        content = content.push(icon(g).size(SMALL).color(color));
    }
    content = content.push(widgets::label(name, SMALL, color));
    container(content)
        .padding([2, 10])
        .style(move |_| iced::widget::container::Style {
            background: Some(tint(color, 0.12).into()),
            border: iced::Border {
                color: tint(color, 0.4),
                width: 1.0,
                radius: 999.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn big_number<'a>(value: String, caption: String) -> Element<'a, Msg> {
    column![
        widgets::label(value, 24.0, TEXT_PRIMARY).font(widgets::BOLD),
        widgets::label(caption, SMALL, TEXT_SECONDARY),
    ]
    .width(Fill)
    .into()
}
