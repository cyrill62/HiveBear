//! The egui application: window shell, navigation and the pages.

mod pages;
mod palette;
mod shared;
mod theme;
mod toast;
mod update;
mod util;
mod welcome;
mod widgets;

use std::sync::Arc;

use eframe::egui::{
    self, Align, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText,
    Sense, Stroke, Vec2,
};
use egui_phosphor::regular as icon;
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::tasks::{Ctx, Task};
use shared::Shared;
use theme::*;
use toast::Toasts;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    Dashboard,
    Models,
    Chat,
    Benchmark,
    Mesh,
    Account,
    Settings,
}

impl Route {
    fn label(self) -> &'static str {
        match self {
            Route::Dashboard => "Dashboard",
            Route::Models => "Models",
            Route::Chat => "Chat",
            Route::Benchmark => "Benchmark",
            Route::Mesh => "Mesh",
            Route::Account => "Account",
            Route::Settings => "Settings",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Route::Dashboard => icon::SQUARES_FOUR,
            Route::Models => icon::MAGNIFYING_GLASS,
            Route::Chat => icon::CHAT_CIRCLE,
            Route::Benchmark => icon::GAUGE,
            Route::Mesh => icon::GRAPH,
            Route::Account => icon::USER_CIRCLE,
            Route::Settings => icon::GEAR,
        }
    }
}

/// Per-install UI preferences, saved by eframe between runs (what the web
/// app kept in `localStorage`).
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub onboarded: bool,
    pub sidebar_collapsed: bool,
    pub first_chat_nudge_dismissed: bool,
}

const PREFS_KEY: &str = "hivebear.prefs.v1";

/// What a page gets to work with each frame.
pub struct Env<'a> {
    pub ctx: &'a Ctx,
    pub shared: &'a mut Shared,
    pub toasts: &'a mut Toasts,
    pub prefs: &'a mut Prefs,
    nav: &'a mut Option<Route>,
}

impl Env<'_> {
    pub fn go(&mut self, route: Route) {
        *self.nav = Some(route);
    }

    pub fn state(&self) -> &Arc<AppState> {
        &self.ctx.state
    }
}

enum Init {
    Starting(Task<Result<AppState, String>>),
    Failed(String),
    Ready(Ctx),
}

pub struct HiveBearApp {
    rt: tokio::runtime::Handle,
    init: Init,
    prefs: Prefs,
    route: Route,
    pages: pages::Pages,
    shared: Shared,
    toasts: Toasts,
    palette: palette::Palette,
    welcome: welcome::Welcome,
    update: update::UpdateNotice,
    logo: egui::TextureHandle,
}

impl HiveBearApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Handle) -> Self {
        theme::install(&cc.egui_ctx);

        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, PREFS_KEY))
            .unwrap_or_default();

        // Build the backend off the UI thread. Hardware profiling (GPU
        // enumeration, a disk read test) and opening the registry take
        // seconds on some machines; the window paints a "Starting…" screen
        // meanwhile instead of appearing frozen.
        let (tx, rx) = tokio::sync::oneshot::channel();
        let init_rt = rt.clone();
        let spawned = std::thread::Builder::new()
            .name("hivebear-init".into())
            .spawn(move || {
                let result = AppState::init_with_paths(hivebear_core::AppPaths::new(), init_rt);
                if let Ok(state) = &result {
                    // Auto-start the mesh only if the user opted in earlier.
                    // Both flags default to false, so a first launch never
                    // binds the QUIC socket (and never triggers a firewall
                    // prompt) before anyone asked for the mesh.
                    let mesh = state.config().mesh;
                    if mesh.enabled && mesh.auto_join {
                        if let Err(e) = state.start_mesh() {
                            tracing::warn!("Failed to auto-start mesh (non-fatal): {e}");
                        }
                    }
                }
                let _ = tx.send(result);
            });
        let init = match spawned {
            Ok(_) => Init::Starting(Task::spawn(&rt, &cc.egui_ctx, async move {
                rx.await
                    .unwrap_or_else(|_| Err("Initialisation stopped unexpectedly.".into()))
            })),
            Err(e) => Init::Failed(format!("Could not start the initialisation thread: {e}")),
        };

        let update = update::UpdateNotice::new(&rt, &cc.egui_ctx);
        let welcome = welcome::Welcome::new(prefs.onboarded);

        Self {
            logo: load_logo(&cc.egui_ctx),
            rt,
            init,
            prefs,
            route: Route::Dashboard,
            pages: pages::Pages::default(),
            shared: Shared::default(),
            toasts: Toasts::default(),
            palette: palette::Palette::default(),
            welcome,
            update,
        }
    }

    fn poll_init(&mut self, ctx: &egui::Context) {
        let Init::Starting(task) = &mut self.init else {
            return;
        };
        match task.poll() {
            None => {}
            Some(Ok(Ok(state))) => {
                let app_ctx = Ctx {
                    rt: self.rt.clone(),
                    egui: ctx.clone(),
                    state: Arc::new(state),
                };
                self.shared.bootstrap(&app_ctx);
                self.init = Init::Ready(app_ctx);
                self.enter(self.route);
            }
            Some(Ok(Err(message))) => {
                crate::write_startup_failure(&message);
                self.init = Init::Failed(message);
            }
            Some(Err(())) => {
                let message = "Initialisation stopped unexpectedly.".to_string();
                crate::write_startup_failure(&message);
                self.init = Init::Failed(message);
            }
        }
    }

    /// Switch page, running its "on open" refresh.
    fn enter(&mut self, route: Route) {
        self.route = route;
        let Init::Ready(ctx) = &self.init else {
            return;
        };
        let mut nav = None;
        let mut env = Env {
            ctx,
            shared: &mut self.shared,
            toasts: &mut self.toasts,
            prefs: &mut self.prefs,
            nav: &mut nav,
        };
        self.pages.enter(route, &mut env);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) -> Option<Route> {
        let mut go = None;
        ctx.input_mut(|i| {
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::K)) {
                self.palette.toggle();
            }
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::N)) {
                self.pages.chat.request_new_chat();
                go = Some(Route::Chat);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Comma)) {
                go = Some(Route::Settings);
            }
        });
        go
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) -> Option<Route> {
        let collapsed = self.prefs.sidebar_collapsed;
        let mut go = None;

        egui::Panel::left("sidebar")
            .exact_size(if collapsed { 56.0 } else { 224.0 })
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| {
                // Logo
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(if collapsed { 6.0 } else { 8.0 });
                    ui.add(
                        egui::Image::new((self.logo.id(), Vec2::splat(28.0)))
                            .corner_radius(CornerRadius::same(RADIUS_MD)),
                    );
                    if !collapsed {
                        ui.label(RichText::new("HiveBear").strong().color(TEXT_PRIMARY));
                    }
                });
                ui.add_space(12.0);

                for route in [Route::Dashboard, Route::Models, Route::Chat] {
                    go = go.or(self.nav_link(ui, route, collapsed));
                }
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                for route in [Route::Benchmark, Route::Mesh] {
                    go = go.or(self.nav_link(ui, route, collapsed));
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let (glyph, tip) = if collapsed {
                            (icon::SIDEBAR_SIMPLE, "Expand sidebar")
                        } else {
                            (icon::SIDEBAR, "Collapse sidebar")
                        };
                        if widgets::icon_button(ui, glyph, tip).clicked() {
                            self.prefs.sidebar_collapsed = !collapsed;
                        }
                        if !collapsed {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                        .size(10.0)
                                        .color(TEXT_MUTED),
                                );
                            });
                        }
                    });
                    ui.separator();
                    for route in [Route::Settings, Route::Account] {
                        go = go.or(self.nav_link(ui, route, collapsed));
                    }
                });
            });
        go
    }

    fn nav_link(&self, ui: &mut egui::Ui, route: Route, collapsed: bool) -> Option<Route> {
        let active = self.route == route;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let response = if collapsed {
            response.on_hover_text(route.label())
        } else {
            response
        };

        let painter = ui.painter();
        if active || response.hovered() {
            let fill = if active {
                SURFACE_OVERLAY
            } else {
                tint(SURFACE_OVERLAY, 0.5)
            };
            painter.rect_filled(rect, CornerRadius::same(RADIUS_MD), fill);
        }
        if active {
            let bar = egui::Rect::from_center_size(
                egui::pos2(rect.left() + 1.5, rect.center().y),
                Vec2::new(3.0, 16.0),
            );
            painter.rect_filled(bar, CornerRadius::same(2), PAW_500);
        }
        let color = if active || response.hovered() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        let icon_x = if collapsed {
            rect.center().x
        } else {
            rect.left() + 20.0
        };
        painter.text(
            egui::pos2(icon_x, rect.center().y),
            egui::Align2::CENTER_CENTER,
            route.icon(),
            egui::FontId::proportional(16.0),
            color,
        );
        if !collapsed {
            painter.text(
                egui::pos2(rect.left() + 38.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                route.label(),
                egui::FontId::proportional(BODY),
                color,
            );
        }
        response.clicked().then_some(route)
    }
}

impl eframe::App for HiveBearApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_init(&ctx);
        self.update.poll(&self.rt, &ctx);

        match &self.init {
            Init::Starting(_) => starting_screen(ui),
            Init::Failed(message) => {
                let message = message.clone();
                failed_screen(ui, &message);
            }
            Init::Ready(app_ctx) => {
                let app_ctx = app_ctx.clone();
                let mut nav = self.shortcuts(&ctx);
                self.shared.poll(&app_ctx, &mut self.toasts, &mut nav);

                if let Some(route) = self.sidebar(ui) {
                    nav = Some(route);
                }

                egui::CentralPanel::default()
                    .frame(Frame::new().fill(SURFACE))
                    .show(ui, |ui| {
                        let mut env = Env {
                            ctx: &app_ctx,
                            shared: &mut self.shared,
                            toasts: &mut self.toasts,
                            prefs: &mut self.prefs,
                            nav: &mut nav,
                        };
                        self.pages.show(self.route, ui, &mut env);
                    });

                if let Some((route, new_chat)) = self.palette.show(&ctx) {
                    if new_chat {
                        self.pages.chat.request_new_chat();
                    }
                    nav = Some(route);
                }
                if let Some(route) = nav {
                    if route != self.route {
                        self.enter(route);
                    }
                }
            }
        }

        // The welcome screen does not need the backend for its first slides,
        // so it sits above the startup gate, as it did in the web app.
        let ready = match &self.init {
            Init::Ready(c) => Some(c.clone()),
            _ => None,
        };
        if let Some(outcome) = self.welcome.show(&ctx, ready.as_ref(), &mut self.shared) {
            self.prefs.onboarded = true;
            if outcome.started_install && self.route != Route::Dashboard {
                // The dashboard is where a download's progress shows.
                self.enter(Route::Dashboard);
            }
        }

        let update_height = self.update.show(&ctx);
        self.toasts.show(&ctx, update_height);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFS_KEY, &self.prefs);
    }

    fn on_exit(&mut self) {
        // Deregister from the coordination server rather than leaving peers
        // to time this node out.
        if let Init::Ready(ctx) = &self.init {
            let state = Arc::clone(&ctx.state);
            let _ = ctx.rt.block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(3), state.stop_mesh()).await
            });
        }
        crate::telemetry::flush();
    }
}

fn starting_screen(ui: &mut egui::Ui) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(SURFACE))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() / 2.0 - 40.0);
                ui.add(egui::Spinner::new().size(22.0).color(PAW_500));
                ui.add_space(6.0);
                ui.label(RichText::new("Starting HiveBear…").color(TEXT_MUTED));
                ui.label(widgets::muted("Checking your CPU, memory and GPU."));
            });
        });
}

fn failed_screen(ui: &mut egui::Ui, message: &str) {
    egui::CentralPanel::default()
        .frame(Frame::new().fill(SURFACE).inner_margin(Margin::same(32)))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.label(RichText::new(icon::WARNING_CIRCLE).size(32.0).color(DANGER));
                ui.label(
                    RichText::new("HiveBear could not start")
                        .size(18.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
                ui.add_space(8.0);
                ui.set_max_width(560.0);
                widgets::error_banner(ui, message);
                ui.add_space(8.0);
                ui.label(widgets::muted(
                    "Details were written to startup-error.log in HiveBear's data folder.",
                ));
                ui.add_space(12.0);
                if widgets::Btn::secondary(icon::X, "Quit").show(ui).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
}

fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    const LOGO: &[u8] = include_bytes!("../../assets/logo.png");
    let image = match image::load_from_memory_with_format(LOGO, image::ImageFormat::Png) {
        Ok(img) => {
            let img = img
                .resize(96, 96, image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())
        }
        Err(e) => {
            tracing::warn!("Could not decode the logo: {e}");
            egui::ColorImage::filled([1, 1], PAW_500)
        }
    };
    ctx.load_texture("logo", image, egui::TextureOptions::LINEAR)
}
