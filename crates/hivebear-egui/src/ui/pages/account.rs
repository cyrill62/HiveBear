use eframe::egui::{
    self, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui,
};
use egui_phosphor::regular as icon;

use crate::backend::account::{
    self as acct, AccountInfo, ApiKeyInfo, AuthResult, NewApiKey, UsageSummary,
};
use crate::error::CmdResult;
use crate::tasks::{poll_slot, Task};
use crate::ui::theme::*;
use crate::ui::util::open_url;
use crate::ui::widgets::{self, Btn, Pad};
use crate::ui::Env;

#[derive(Default, PartialEq)]
enum View {
    #[default]
    Choice,
    Email,
}

#[derive(Default)]
pub struct Account {
    account: Option<AccountInfo>,
    loaded: bool,
    account_task: Option<Task<CmdResult<Option<AccountInfo>>>>,
    auth_task: Option<Task<CmdResult<AuthResult>>>,
    logout_task: Option<Task<CmdResult<()>>>,
    error: Option<String>,

    view: View,
    register: bool,
    email: String,
    password: String,
    display_name: String,
    show_password: bool,

    usage: Option<UsageSummary>,
    usage_task: Option<Task<CmdResult<UsageSummary>>>,
    keys: Vec<ApiKeyInfo>,
    keys_task: Option<Task<CmdResult<Vec<ApiKeyInfo>>>>,
    new_key_label: String,
    /// A freshly created key, shown once so it can be copied.
    new_key: Option<String>,
    create_key_task: Option<Task<CmdResult<NewApiKey>>>,
    revoke_task: Option<Task<CmdResult<()>>>,
    checkout_task: Option<Task<CmdResult<String>>>,
}

impl Account {
    pub fn enter(&mut self, env: &mut Env) {
        self.refresh(env);
    }

    fn refresh(&mut self, env: &Env) {
        self.account_task = Some(env.ctx.run(|s| async move { acct::get_account(&s).await }));
    }

    fn refresh_details(&mut self, env: &Env) {
        self.usage_task = Some(
            env.ctx
                .run(|s| async move { acct::get_usage_summary(&s).await }),
        );
        self.keys_task = Some(
            env.ctx
                .run(|s| async move { acct::list_api_keys(&s).await }),
        );
    }

    pub fn poll(&mut self, env: &mut Env) {
        if let Some(result) = poll_slot(&mut self.account_task) {
            self.loaded = true;
            // Offline or not authenticated — both are fine.
            self.account = result.ok().flatten();
            if self.account.is_some() {
                self.refresh_details(env);
            }
        }
        if let Some(result) = poll_slot(&mut self.auth_task) {
            match result {
                Ok(_) => {
                    self.error = None;
                    self.password.clear();
                    self.refresh(env);
                }
                Err(e) => self.error = Some(e),
            }
        }
        if poll_slot(&mut self.logout_task).is_some() {
            self.account = None;
            self.usage = None;
            self.keys.clear();
            self.new_key = None;
        }
        if let Some(result) = poll_slot(&mut self.usage_task) {
            // Offline — the section says so.
            self.usage = result.ok();
        }
        if let Some(Ok(keys)) = poll_slot(&mut self.keys_task) {
            self.keys = keys;
        }
        if let Some(result) = poll_slot(&mut self.create_key_task) {
            match result {
                Ok(key) => {
                    self.new_key = Some(key.key);
                    self.new_key_label.clear();
                    self.refresh_details(env);
                }
                Err(e) => env.toasts.error(e),
            }
        }
        if let Some(result) = poll_slot(&mut self.revoke_task) {
            match result {
                Ok(()) => self.refresh_details(env),
                Err(e) => env.toasts.error(e),
            }
        }
        if let Some(result) = poll_slot(&mut self.checkout_task) {
            match result {
                Ok(url) => open_url(&env.ctx.egui, &url),
                Err(e) => env.toasts.error(e),
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        if !self.loaded {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                widgets::spinner(ui);
                ui.label(widgets::muted("Loading account..."));
            });
            return;
        }

        if let Some(account) = self.account.clone() {
            ui.vertical_centered(|ui| {
                ui.set_max_width(672.0);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    widgets::page_title(ui, "Account");
                    self.dashboard(ui, env, &account);
                });
            });
            return;
        }

        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            match self.view {
                View::Choice => self.auth_choice(ui, env),
                View::Email => self.email_form(ui, env),
            }
        });
    }

    fn auth_choice(&mut self, ui: &mut Ui, env: &mut Env) {
        let submitting = self.auth_task.is_some();
        ui.set_max_width(672.0);
        ui.label(widgets::text("Choose how to sign in").size(20.0).strong());
        ui.label(widgets::secondary(
            "Both paths give you the same features. Pick what suits you.",
        ));
        ui.add_space(12.0);
        if let Some(e) = &self.error {
            widgets::error_banner(ui, e);
        }
        ui.columns(2, |cols| {
            if choice_card(
                &mut cols[0],
                icon::ENVELOPE,
                "Email Account",
                "For teams & billing management. Password recovery included.",
                !submitting,
            ) {
                self.view = View::Email;
                self.error = None;
            }
            if choice_card(
                &mut cols[1],
                icon::FINGERPRINT,
                "Device Key",
                "Maximum privacy. No email required. Mullvad-style anonymous.",
                !submitting,
            ) {
                self.error = None;
                self.auth_task = Some(
                    env.ctx
                        .run(|s| async move { acct::activate_device(&s).await }),
                );
            }
        });
        if submitting {
            widgets::spinner(ui);
        }
    }

    fn email_form(&mut self, ui: &mut Ui, env: &mut Env) {
        ui.set_max_width(384.0);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            if widgets::link_button(ui, &format!("{}  Back", icon::ARROW_LEFT), TEXT_SECONDARY)
                .clicked()
            {
                self.view = View::Choice;
                self.error = None;
            }
            if let Some(tab) =
                widgets::segmented(ui, &["Sign In", "Register"], usize::from(self.register))
            {
                self.register = tab == 1;
            }
            if let Some(e) = &self.error {
                widgets::error_banner(ui, e);
            }
            let width = ui.available_width();
            if self.register {
                widgets::text_field(ui, &mut self.display_name, "Display name", width);
            }
            widgets::text_field(ui, &mut self.email, "Email", width);
            ui.horizontal(|ui| {
                let field = widgets::password_field(
                    ui,
                    &mut self.password,
                    "Password (min 8 characters)",
                    self.show_password,
                    width - 36.0,
                );
                let glyph = if self.show_password {
                    icon::EYE_SLASH
                } else {
                    icon::EYE
                };
                if widgets::icon_button(ui, glyph, "Show password").clicked() {
                    self.show_password = !self.show_password;
                }
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.submit(env);
                }
            });

            let label = if self.register {
                "Create Account"
            } else {
                "Sign In"
            };
            let response = ui.add_sized(
                [width, 34.0],
                egui::Button::new(RichText::new(label).color(Color32::WHITE))
                    .fill(PAW_500)
                    .corner_radius(CornerRadius::same(RADIUS_MD)),
            );
            if self.auth_task.is_some() {
                widgets::spinner(ui);
            } else if response.clicked() {
                self.submit(env);
            }
        });
    }

    fn submit(&mut self, env: &Env) {
        if self.auth_task.is_some() {
            return;
        }
        let email = self.email.trim().to_string();
        let name = self.display_name.trim().to_string();
        if email.is_empty() || !email.contains('@') {
            self.error = Some("Enter a valid email address.".into());
            return;
        }
        if self.password.chars().count() < 8 {
            self.error = Some("Password must be at least 8 characters.".into());
            return;
        }
        if self.register && name.is_empty() {
            self.error = Some("Enter a display name.".into());
            return;
        }
        self.error = None;
        let password = self.password.clone();
        self.auth_task = Some(if self.register {
            env.ctx
                .run(move |s| async move { acct::register(&s, &email, &password, &name).await })
        } else {
            env.ctx
                .run(move |s| async move { acct::login(&s, &email, &password).await })
        });
    }

    fn dashboard(&mut self, ui: &mut Ui, env: &mut Env, account: &AccountInfo) {
        // Identity
        widgets::card(ui, Pad::Lg, |ui| {
            ui.horizontal(|ui| {
                let glyph = if account.auth_mode == "device" {
                    icon::FINGERPRINT
                } else {
                    icon::USER_CIRCLE
                };
                ui.label(RichText::new(glyph).size(24.0).color(PAW_500));
                ui.vertical(|ui| {
                    if account.auth_mode == "email" {
                        ui.label(
                            widgets::text(account.display_name.as_deref().unwrap_or_default())
                                .strong(),
                        );
                        ui.label(widgets::secondary(
                            account.email.as_deref().unwrap_or_default(),
                        ));
                    } else {
                        ui.label(widgets::text("Anonymous Device").strong());
                        let key: String = account
                            .pubkey
                            .as_deref()
                            .unwrap_or_default()
                            .chars()
                            .take(16)
                            .collect();
                        ui.label(
                            RichText::new(format!("{key}..."))
                                .font(mono(SMALL))
                                .color(TEXT_SECONDARY),
                        );
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::link_button(ui, "Sign out", TEXT_SECONDARY).clicked()
                        && self.logout_task.is_none()
                    {
                        self.logout_task =
                            Some(env.ctx.run(|s| async move { acct::logout(&s).await }));
                    }
                    tier_badge(ui, &account.tier);
                });
            });
        });

        // Subscription
        widgets::card(ui, Pad::Lg, |ui| {
            ui.label(widgets::secondary("Subscription").strong());
            if account.tier == "community" {
                ui.label(widgets::text(
                    "You're on the Community plan. Upgrade for priority mesh, private hives, and unlimited cloud.",
                ));
                ui.horizontal(|ui| {
                    let busy = self.checkout_task.is_some();
                    let mut plan = None;
                    if Btn::primary("", "Upgrade to Pro — $15/mo")
                        .enabled(!busy)
                        .show(ui)
                        .clicked()
                    {
                        plan = Some("pro");
                    }
                    if account.auth_mode == "email"
                        && Btn::secondary("", "Team — $10/seat/mo")
                            .enabled(!busy)
                            .show(ui)
                            .clicked()
                    {
                        plan = Some("team");
                    }
                    if let Some(plan) = plan {
                        self.checkout_task =
                            Some(env.ctx.run(move |s| async move {
                                acct::create_checkout(&s, plan).await
                            }));
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    tier_badge(ui, &account.tier);
                    ui.label(widgets::text("plan active"));
                    if let Some(end) = &account.current_period_end {
                        let date = end.split('T').next().unwrap_or(end);
                        ui.label(widgets::secondary(format!("· renews {date}")));
                    }
                });
            }
        });

        // Usage
        widgets::card(ui, Pad::Lg, |ui| {
            ui.label(widgets::secondary("Usage").strong());
            if self.usage_task.is_some() {
                ui.label(widgets::muted("Loading..."));
            } else if let Some(u) = &self.usage {
                ui.columns(3, |cols| {
                    let limit = u
                        .cloud_requests_limit
                        .map(|l| format!(" / {l}"))
                        .unwrap_or_default();
                    big_number(
                        &mut cols[0],
                        &u.cloud_requests_today.to_string(),
                        &format!("Cloud requests today{limit}"),
                    );
                    big_number(
                        &mut cols[1],
                        &format!("{}m", (u.mesh_seconds_contributed as f64 / 60.0).round()),
                        "Mesh contributed",
                    );
                    big_number(
                        &mut cols[2],
                        &u.model_downloads_this_month.to_string(),
                        "Downloads this month",
                    );
                });
            } else {
                ui.label(widgets::muted("Offline — usage data unavailable"));
            }
        });

        // API keys
        widgets::card(ui, Pad::Lg, |ui| {
            ui.horizontal(|ui| {
                ui.label(widgets::secondary("API Keys").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let busy = self.create_key_task.is_some();
                    if Btn::primary(icon::PLUS, "Create")
                        .small()
                        .busy(busy)
                        .show(ui)
                        .clicked()
                    {
                        let label = match self.new_key_label.trim() {
                            "" => "default".to_string(),
                            l => l.to_string(),
                        };
                        self.create_key_task =
                            Some(env.ctx.run(move |s| async move {
                                acct::create_api_key(&s, &label).await
                            }));
                    }
                    widgets::text_field(ui, &mut self.new_key_label, "Label", 140.0);
                });
            });

            if let Some(key) = &self.new_key {
                Frame::new()
                    .fill(tint(SUCCESS, 0.1))
                    .stroke(Stroke::new(1.0, tint(SUCCESS, 0.3)))
                    .corner_radius(CornerRadius::same(RADIUS_MD))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(
                                "New API key created — copy it now, it won't be shown again:",
                            )
                            .size(SMALL)
                            .color(SUCCESS),
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(key).font(mono(SMALL)).color(TEXT_PRIMARY),
                                )
                                .wrap(),
                            );
                            if widgets::icon_button(ui, icon::COPY, "Copy").clicked() {
                                ui.ctx().copy_text(key.clone());
                                env.toasts.success("Copied to clipboard.");
                            }
                        });
                    });
            }

            if self.keys_task.is_some() && self.keys.is_empty() {
                ui.label(widgets::muted("Loading..."));
            } else if self.keys.is_empty() {
                ui.label(widgets::muted("No API keys yet."));
            }
            let mut revoke = None;
            for key in &self.keys {
                Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(RADIUS_MD))
                    .inner_margin(Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}...", key.key_prefix))
                                    .font(mono(SMALL))
                                    .color(TEXT_PRIMARY),
                            );
                            ui.label(widgets::secondary(&key.label).size(SMALL));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if widgets::icon_button(ui, icon::TRASH, "Revoke").clicked() {
                                    revoke = Some(key.id.clone());
                                }
                            });
                        });
                    });
            }
            if let (Some(id), None) = (revoke, &self.revoke_task) {
                self.revoke_task = Some(
                    env.ctx
                        .run(move |s| async move { acct::revoke_api_key(&s, &id).await }),
                );
            }
        });
    }
}

fn choice_card(ui: &mut Ui, glyph: &str, title: &str, desc: &str, enabled: bool) -> bool {
    let response = widgets::clickable_card(ui, title, Pad::Lg, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(glyph).size(32.0).color(PAW_500));
            ui.label(widgets::text(title).strong());
            ui.add(egui::Label::new(widgets::secondary(desc).size(SMALL)).wrap());
        });
    });
    enabled && response.clicked()
}

fn tier_badge(ui: &mut Ui, tier: &str) {
    let (color, glyph) = match tier {
        "pro" => (Color32::from_rgb(0xfc, 0xd3, 0x4d), Some(icon::SHIELD)),
        "team" => (Color32::from_rgb(0x93, 0xc5, 0xfd), Some(icon::KEY)),
        "enterprise" => (Color32::from_rgb(0xd8, 0xb4, 0xfe), Some(icon::CROWN)),
        _ => (Color32::from_rgb(0xd4, 0xd4, 0xd8), None),
    };
    let mut label: String = tier
        .chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .chain(tier.chars().skip(1))
        .collect();
    if let Some(g) = glyph {
        label = format!("{g} {label}");
    }
    Frame::new()
        .fill(tint(color, 0.12))
        .stroke(Stroke::new(1.0, tint(color, 0.4)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(SMALL).color(color));
        });
}

fn big_number(ui: &mut Ui, value: &str, label: &str) {
    ui.label(RichText::new(value).size(24.0).strong().color(TEXT_PRIMARY));
    ui.label(widgets::secondary(label).size(SMALL));
}
