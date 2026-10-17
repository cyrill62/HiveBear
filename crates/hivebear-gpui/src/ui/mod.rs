//! The GPUI application, built on Longbridge's GPUI Kit.
//!
//! The shell owns navigation, the overlays (welcome, command palette,
//! update notice) and one entity per page. Backend work runs on Tokio (see
//! [`cmd`] and [`blocking`]) and comes back into GPUI tasks, which update the
//! entities and let the window re-render.

mod assets;
mod pages;
mod palette;
mod shared;
mod theme;
mod util;
mod welcome;
mod widgets;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde::{Deserialize, Serialize};

use crate::backend;
use crate::error::CmdResult;
use crate::state::AppState;
use pages::Pages;
use shared::Shared;
use theme::*;

actions!(hivebear, [OpenPalette, NewChat, OpenSettings]);

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

    fn icon(self) -> IconName {
        match self {
            Route::Dashboard => IconName::LayoutDashboard,
            Route::Models => IconName::Search,
            Route::Chat => IconName::MessageSquare,
            Route::Benchmark => IconName::Gauge,
            Route::Mesh => IconName::Network,
            Route::Account => IconName::CircleUser,
            Route::Settings => IconName::Settings,
        }
    }
}

/// Emitted by pages (and the shared cache) to change page.
pub struct Navigate(pub Route);

// ── Globals ─────────────────────────────────────────────────────────

/// The Tokio runtime the backend runs on.
pub struct Backend {
    pub rt: tokio::runtime::Handle,
}

impl Global for Backend {}

pub fn rt(cx: &App) -> tokio::runtime::Handle {
    cx.global::<Backend>().rt.clone()
}

/// Notifications queued from places without a `Window`; the shell shows them
/// on its next render.
#[derive(Default)]
struct ToastQueue(Vec<Notification>);

impl Global for ToastQueue {}

pub fn toast_ok(cx: &mut App, message: impl Into<SharedString>) {
    cx.default_global::<ToastQueue>()
        .0
        .push(Notification::success(message));
    cx.refresh_windows();
}

pub fn toast_err(cx: &mut App, message: impl Into<SharedString>) {
    let message = message.into();
    tracing::warn!("{message}");
    cx.default_global::<ToastQueue>()
        .0
        .push(Notification::error(message));
    cx.refresh_windows();
}

pub fn copy(cx: &mut App, text: String) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    toast_ok(cx, "Copied to clipboard.");
}

/// Run a backend call on Tokio and await it from a GPUI task. A task that
/// panicked (reported by the Sentry panic hook) becomes an error.
pub async fn cmd<T: Send + 'static>(
    rt: tokio::runtime::Handle,
    fut: impl Future<Output = CmdResult<T>> + Send + 'static,
) -> CmdResult<T> {
    rt.spawn(fut)
        .await
        .unwrap_or_else(|e| Err(format!("Background task failed: {e}")))
}

/// Run a backend call that cannot fail.
pub async fn call<T: Send + 'static>(
    rt: tokio::runtime::Handle,
    fut: impl Future<Output = T> + Send + 'static,
) -> Option<T> {
    match rt.spawn(fut).await {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::error!("Background task failed: {e}");
            None
        }
    }
}

/// Run blocking backend work (SQLite, config files, the keychain) on Tokio's
/// blocking pool, so a slow disk never stalls a frame.
pub async fn blocking<T: Send + 'static>(
    rt: tokio::runtime::Handle,
    state: Arc<AppState>,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    cmd(rt, async move {
        tokio::task::spawn_blocking(move || f(&state))
            .await
            .unwrap_or_else(|e| Err(format!("Background task failed: {e}")))
    })
    .await
}

// ── Preferences ─────────────────────────────────────────────────────

/// Per-install UI preferences (what the web app kept in `localStorage`).
#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub onboarded: bool,
    pub sidebar_collapsed: bool,
    pub first_chat_nudge_dismissed: bool,
}

impl Global for Prefs {}

impl Prefs {
    fn path() -> PathBuf {
        hivebear_core::AppPaths::new()
            .data_dir
            .join("ui-prefs.json")
    }

    fn load() -> Self {
        std::fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::path();
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::write(&path, serde_json::to_vec_pretty(self).unwrap_or_default())
            });
        if let Err(e) = result {
            tracing::warn!("Could not save UI preferences: {e}");
        }
    }

    /// Change the preferences and persist them.
    pub fn update(cx: &mut App, f: impl FnOnce(&mut Prefs)) {
        let prefs = cx.global_mut::<Prefs>();
        f(prefs);
        prefs.save();
    }
}

// ── Application ─────────────────────────────────────────────────────

const LOGO: &[u8] = include_bytes!("../../assets/logo.png");
const UPDATE_RECHECK: Duration = Duration::from_secs(4 * 60 * 60);
const LIVE_REFRESH: Duration = Duration::from_secs(5);

pub fn run(rt: tokio::runtime::Handle) {
    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            theme::apply(cx);
            cx.set_global(Backend { rt });
            cx.set_global(Prefs::load());
            cx.bind_keys([
                KeyBinding::new("secondary-k", OpenPalette, None),
                KeyBinding::new("secondary-n", NewChat, None),
                KeyBinding::new("secondary-,", OpenSettings, None),
            ]);
            cx.on_window_closed(|cx, _| cx.quit()).detach();

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1200.), px(800.)), cx)),
                window_min_size: Some(size(px(900.), px(600.))),
                app_id: Some("com.hivebear.app".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some("HiveBear".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if let Err(e) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Shell::new(window, cx))
            }) {
                let message = format!("Could not open the window.\n\n{e}");
                tracing::error!("{message}");
                crate::write_startup_failure(&message);
                cx.quit();
            }
        });
}

enum Init {
    Starting,
    Failed(String),
    Ready(Arc<AppState>),
}

pub struct Shell {
    init: Init,
    route: Route,
    pages: Option<Pages>,
    welcome: Option<Entity<welcome::Welcome>>,
    /// A welcome-screen choice made before the backend was ready.
    pending_usage_choice: Option<bool>,
    update_available: Option<String>,
    update_dismissed: bool,
    focus: FocusHandle,
    logo: Arc<Image>,
    _tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rt = rt(cx);

        // Build the backend off the UI thread. Hardware profiling (GPU
        // enumeration, a disk read test) and opening the registry take
        // seconds on some machines; the window shows "Starting…" meanwhile.
        let boot = cx.spawn_in(window, {
            let rt = rt.clone();
            async move |this, cx| {
                let result = init_backend(rt).await;
                this.update_in(cx, |this, window, cx| this.booted(result, window, cx))
                    .ok();
            }
        });

        // Update check now and every four hours after.
        let updates = cx.spawn(async move |this, cx| loop {
            let client = reqwest::Client::new();
            let result = call(
                rt.clone(),
                async move { backend::update::check(&client).await },
            )
            .await;
            match result {
                Some(Ok(version)) => {
                    let alive = this
                        .update(cx, |this, cx| {
                            this.update_available = version;
                            cx.notify();
                        })
                        .is_ok();
                    if !alive {
                        return;
                    }
                }
                // Non-fatal for the user, but not something to swallow: a
                // broken update check is invisible by design, and v0.1.5 sat
                // un-updatable for six months because nothing reported this.
                Some(Err(e)) => crate::telemetry::report_error(&e, "updater"),
                None => {}
            }
            cx.background_executor().timer(UPDATE_RECHECK).await;
        });

        // The dashboard and mesh pages read live, in-memory mesh status;
        // repaint them periodically so peers arriving show up.
        let live = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(LIVE_REFRESH).await;
            let alive = this
                .update(cx, |this, cx| {
                    if matches!(this.route, Route::Dashboard | Route::Mesh) {
                        cx.notify();
                    }
                })
                .is_ok();
            if !alive {
                return;
            }
        });

        // Deregister from the coordination server before the window goes,
        // rather than leaving peers to time this node out.
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            if let Some(state) = weak.upgrade().and_then(|s| s.read(cx).ready_state()) {
                let handle = self::rt(cx);
                handle.block_on(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(3), state.stop_mesh()).await;
                });
            }
            crate::telemetry::flush();
            true
        });

        cx.defer_in(window, |this: &mut Self, window, cx| {
            this.focus.focus(window, cx);
            if !cx.global::<Prefs>().onboarded {
                this.open_welcome(window, cx);
            }
        });

        Self {
            init: Init::Starting,
            route: Route::Dashboard,
            pages: None,
            welcome: None,
            pending_usage_choice: None,
            update_available: None,
            update_dismissed: false,
            focus: cx.focus_handle(),
            logo: Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec())),
            _tasks: vec![boot, updates, live],
            _subscriptions: vec![],
        }
    }

    fn ready_state(&self) -> Option<Arc<AppState>> {
        match &self.init {
            Init::Ready(state) => Some(Arc::clone(state)),
            _ => None,
        }
    }

    fn booted(
        &mut self,
        result: Result<Arc<AppState>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(state) => {
                let shared = cx.new(|cx| Shared::new(Arc::clone(&state), cx));
                let (pages, subscriptions) = Pages::new(&state, &shared, window, cx);
                self._subscriptions.extend(subscriptions);
                if let Some(welcome) = &self.welcome {
                    welcome.update(cx, |w, cx| {
                        w.set_ready(Arc::clone(&state), shared.clone(), cx)
                    });
                }
                if let Some(choice) = self.pending_usage_choice.take() {
                    welcome::save_onboarding_choices(Arc::clone(&state), choice, cx);
                }
                self.pages = Some(pages);
                self.init = Init::Ready(state);
                self.enter(self.route, window, cx);
            }
            Err(message) => {
                crate::write_startup_failure(&message);
                self.init = Init::Failed(message);
            }
        }
        cx.notify();
    }

    fn navigate(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        if route != self.route {
            self.enter(route, window, cx);
        }
        cx.notify();
    }

    /// Switch page, running its "on open" refresh.
    fn enter(&mut self, route: Route, window: &mut Window, cx: &mut Context<Self>) {
        self.route = route;
        if let Some(pages) = &self.pages {
            pages.enter(route, window, cx);
        }
    }

    fn open_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let welcome = cx.new(welcome::Welcome::new);
        if let Some(state) = self.ready_state() {
            if let Some(pages) = &self.pages {
                let shared = pages.shared.clone();
                welcome.update(cx, |w, cx| w.set_ready(state, shared, cx));
            }
        }
        let sub = cx.subscribe_in(
            &welcome,
            window,
            |this, _, done: &welcome::Finished, window, cx| {
                window.close_dialog(cx);
                this.welcome = None;
                Prefs::update(cx, |p| p.onboarded = true);
                match this.ready_state() {
                    Some(state) => welcome::save_onboarding_choices(state, done.usage_events, cx),
                    None => this.pending_usage_choice = Some(done.usage_events),
                }
                if done.started_install {
                    // The dashboard is where a download's progress shows.
                    this.navigate(Route::Dashboard, window, cx);
                }
            },
        );
        self._subscriptions.push(sub);
        self.welcome = Some(welcome.clone());
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(px(440.))
                .close_button(false)
                .overlay_closable(false)
                .keyboard(false)
                .child(welcome.clone())
        });
    }

    fn open_palette(&mut self, _: &OpenPalette, window: &mut Window, cx: &mut Context<Self>) {
        if self.pages.is_none() || window.has_active_dialog(cx) {
            return;
        }
        let palette = cx.new(|cx| palette::Palette::new(window, cx));
        let sub = cx.subscribe_in(
            &palette,
            window,
            |this, _, choice: &palette::Choice, window, cx| {
                window.close_dialog(cx);
                if choice.new_chat {
                    if let Some(pages) = &this.pages {
                        pages.chat.update(cx, |chat, cx| chat.request_new_chat(cx));
                    }
                }
                this.navigate(choice.route, window, cx);
            },
        );
        self._subscriptions.push(sub);
        palette.update(cx, |p, cx| p.focus(window, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(px(440.))
                .close_button(false)
                .child(palette.clone())
        });
    }

    fn new_chat(&mut self, _: &NewChat, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pages) = &self.pages {
            pages.chat.update(cx, |chat, cx| chat.request_new_chat(cx));
            self.navigate(Route::Chat, window, cx);
        }
    }

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        if self.pages.is_some() {
            self.navigate(Route::Settings, window, cx);
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = cx.global::<Prefs>().sidebar_collapsed;
        let link = |route: Route, cx: &mut Context<Self>| {
            let active = self.route == route;
            div()
                .id(route.label())
                .relative()
                .h(px(34.))
                .w_full()
                .flex()
                .items_center()
                .gap_3()
                .px(px(12.))
                .when(collapsed, |d| d.justify_center())
                .rounded(px(8.))
                .text_sm()
                .cursor_pointer()
                .text_color(c(if active { TEXT_PRIMARY } else { TEXT_SECONDARY }))
                .when(active, |d| d.bg(c(SURFACE_OVERLAY)))
                .hover(|s| s.bg(ca(SURFACE_OVERLAY, 0.6)).text_color(c(TEXT_PRIMARY)))
                .when(active, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(9.))
                            .w(px(3.))
                            .h(px(16.))
                            .rounded(px(2.))
                            .bg(c(PAW_500)),
                    )
                })
                .child(Icon::new(route.icon()).size(px(16.)))
                .when(!collapsed, |d| d.child(route.label()))
                .on_click(cx.listener(move |this, _, window, cx| this.navigate(route, window, cx)))
        };

        let toggle = div()
            .id("sidebar-toggle")
            .p_2()
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(c(TEXT_MUTED))
            .hover(|s| s.bg(c(SURFACE_OVERLAY)).text_color(c(TEXT_PRIMARY)))
            .child(
                Icon::new(if collapsed {
                    IconName::PanelLeftOpen
                } else {
                    IconName::PanelLeftClose
                })
                .size(px(14.)),
            )
            .on_click(cx.listener(|_, _, _, cx| {
                Prefs::update(cx, |p| p.sidebar_collapsed = !p.sidebar_collapsed);
                cx.notify();
            }));

        v_flex()
            .h_full()
            .w(px(if collapsed { 56. } else { 224. }))
            .flex_shrink_0()
            .px_2()
            .bg(c(SURFACE))
            .border_r_1()
            .border_color(c(BORDER))
            .child(
                h_flex()
                    .h(px(56.))
                    .px_2()
                    .gap_2()
                    .child(img(self.logo.clone()).size(px(28.)).rounded(px(8.)))
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("HiveBear"),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap_0p5()
                    .child(link(Route::Dashboard, cx))
                    .child(link(Route::Models, cx))
                    .child(link(Route::Chat, cx)),
            )
            .child(div().my_2().mx_1().h(px(1.)).bg(c(BORDER)))
            .child(
                v_flex()
                    .gap_0p5()
                    .child(link(Route::Benchmark, cx))
                    .child(link(Route::Mesh, cx)),
            )
            .child(div().flex_1())
            .child(
                v_flex()
                    .gap_0p5()
                    .pb_1()
                    .child(link(Route::Account, cx))
                    .child(link(Route::Settings, cx)),
            )
            .child(
                h_flex()
                    .justify_between()
                    .py_2()
                    .border_t_1()
                    .border_color(c(BORDER))
                    .child(toggle)
                    .when(!collapsed, |d| {
                        d.child(
                            div()
                                .pr_1()
                                .text_size(px(10.))
                                .text_color(c(TEXT_MUTED))
                                .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                        )
                    }),
            )
    }

    fn update_card(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let version = self
            .update_available
            .clone()
            .filter(|_| !self.update_dismissed)?;
        Some(
            div().absolute().bottom_4().right_4().child(
                widgets::raised_card().min_w(px(300.)).child(
                    h_flex()
                        .gap_3()
                        .child(
                            Icon::new(IconName::Download)
                                .size(px(16.))
                                .text_color(c(PAW_400)),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child("Update available"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(c(TEXT_MUTED))
                                        .child(format!("v{version}")),
                                ),
                        )
                        .child(
                            widgets::button("update-download", "Download")
                                .primary()
                                .small()
                                .on_click(|_, _, _| util::open_url(backend::update::RELEASES_URL)),
                        )
                        .child(
                            widgets::icon_button("update-dismiss", IconName::X, "Dismiss")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.update_dismissed = true;
                                    cx.notify();
                                })),
                        ),
                ),
            ),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for note in std::mem::take(&mut cx.default_global::<ToastQueue>().0) {
            window.push_notification(note, cx);
        }

        let body = match &self.init {
            Init::Starting => starting_screen().into_any_element(),
            Init::Failed(message) => failed_screen(message.clone()).into_any_element(),
            Init::Ready(_) => {
                let page = self.pages.as_ref().map(|p| p.view(self.route));
                h_flex()
                    .size_full()
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .overflow_hidden()
                            .children(page),
                    )
                    .into_any_element()
            }
        };

        div()
            .size_full()
            .relative()
            .bg(c(SURFACE))
            .text_color(c(TEXT_PRIMARY))
            .font_family(cx.theme().font_family.clone())
            .track_focus(&self.focus)
            .key_context("HiveBear")
            .on_action(cx.listener(Self::open_palette))
            .on_action(cx.listener(Self::new_chat))
            .on_action(cx.listener(Self::open_settings))
            .child(body)
            .children(self.update_card(cx))
    }
}

fn starting_screen() -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .child(Spinner::new().large().color(c(PAW_500)))
        .child(
            div()
                .text_sm()
                .text_color(c(TEXT_SECONDARY))
                .child("Starting HiveBear…"),
        )
        .child(
            div()
                .text_xs()
                .text_color(c(TEXT_MUTED))
                .child("Checking your CPU, memory and GPU."),
        )
}

fn failed_screen(message: String) -> impl IntoElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .p_8()
        .child(
            Icon::new(IconName::CircleAlert)
                .size(px(32.))
                .text_color(c(DANGER)),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child("HiveBear could not start"),
        )
        .child(
            div()
                .max_w(px(560.))
                .child(widgets::banner(DANGER, message)),
        )
        .child(
            div()
                .text_xs()
                .text_color(c(TEXT_MUTED))
                .child("Details were written to startup-error.log in HiveBear's data folder."),
        )
        .child(
            widgets::button("quit", "Quit")
                .outline()
                .on_click(|_, _, cx| cx.quit()),
        )
}

async fn init_backend(rt: tokio::runtime::Handle) -> Result<Arc<AppState>, String> {
    let handle = rt.clone();
    let init = rt.spawn_blocking(move || {
        let state = AppState::init_with_paths(hivebear_core::AppPaths::new(), handle)?;
        // Auto-start the mesh only if the user opted in earlier. Both flags
        // default to false, so a first launch never binds the QUIC socket (and
        // never triggers a firewall prompt) before anyone asked for the mesh.
        let mesh = state.config().mesh;
        if mesh.enabled && mesh.auto_join {
            if let Err(e) = state.start_mesh() {
                tracing::warn!("Failed to auto-start mesh (non-fatal): {e}");
            }
        }
        Ok(Arc::new(state))
    });
    init.await
        .unwrap_or_else(|e| Err(format!("Initialisation stopped unexpectedly: {e}")))
}
