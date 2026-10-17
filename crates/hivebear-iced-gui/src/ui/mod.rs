//! The iced application: shell, navigation, overlays and the pages.
//!
//! Elm-style: [`App::update`] is the only place state changes, every backend
//! call is a [`Task`] that comes back as a [`Message`], and timers, keys and
//! window events arrive through [`App::subscription`].

pub mod icon;
mod pages;
mod palette;
mod shared;
pub mod theme;
mod toast;
mod update;
mod util;
mod welcome;
mod widgets;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::keyboard::{self, key::Named, Key};
use iced::widget::{
    button, column, container, image, mouse_area, opaque, row, rule, scrollable, space, stack, text,
};
use iced::{window, Alignment, Element, Fill, Length, Subscription, Task};
use serde::{Deserialize, Serialize};

use crate::error::CmdResult;
use crate::state::AppState;
use icon::icon;
use shared::Shared;
use theme::*;
use toast::Toasts;
use widgets::Kind;

const LOGO: &[u8] = include_bytes!("../../assets/logo.png");

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

    fn glyph(self) -> &'static str {
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

/// Per-install UI preferences (what the web app kept in `localStorage`).
#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub onboarded: bool,
    pub sidebar_collapsed: bool,
    pub first_chat_nudge_dismissed: bool,
}

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
}

#[derive(Clone)]
pub enum Message {
    Booted(Result<Arc<AppState>, String>),
    Navigate(Route),
    ToggleSidebar,
    Key(keyboard::Event),
    Tick,
    DismissToast(u64),
    CloseRequested,
    Exit,
    Quit,

    Shared(shared::Msg),
    Welcome(welcome::Msg),
    Palette(palette::Msg),
    Update(update::Msg),

    Dashboard(pages::dashboard::Msg),
    Models(pages::models::Msg),
    Chat(pages::chat::Msg),
    Benchmark(pages::benchmark::Msg),
    Mesh(pages::mesh::Msg),
    Account(pages::account::Msg),
    Settings(pages::settings::Msg),
}

/// What a page's `update` gets to work with.
pub struct Env<'a> {
    pub state: &'a Arc<AppState>,
    pub shared: &'a mut Shared,
    pub toasts: &'a mut Toasts,
    pub prefs: &'a mut Prefs,
    nav: Option<Route>,
    tasks: Vec<Task<Message>>,
}

impl Env<'_> {
    pub fn go(&mut self, route: Route) {
        self.nav = Some(route);
    }

    /// Run an app-level task (a download, a model load, a clipboard write).
    pub fn spawn(&mut self, task: Task<Message>) {
        self.tasks.push(task);
    }

    pub fn copy(&mut self, text: String) {
        self.tasks.push(iced::clipboard::write(text));
        self.toasts.success("Copied to clipboard.");
    }
}

/// What a page's `view` reads.
pub struct Cx<'a> {
    pub state: &'a AppState,
    pub shared: &'a Shared,
}

/// Run an async backend call and map its result to a message.
pub fn call<T, M, Fut>(
    state: &Arc<AppState>,
    f: impl FnOnce(Arc<AppState>) -> Fut,
    map: impl FnOnce(T) -> M + Send + 'static,
) -> Task<M>
where
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
    M: Send + 'static,
{
    Task::perform(f(Arc::clone(state)), map)
}

/// Run blocking backend work (SQLite, config files, the keychain) on the
/// blocking pool, so a slow disk never stalls the UI.
pub fn blocking<T, M>(
    state: &Arc<AppState>,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
    map: impl FnOnce(CmdResult<T>) -> M + Send + 'static,
) -> Task<M>
where
    T: Send + 'static,
    M: Send + 'static,
{
    let state = Arc::clone(state);
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || f(&state))
                .await
                .unwrap_or_else(|e| Err(format!("Background task failed: {e}")))
        },
        map,
    )
}

enum Init {
    Starting,
    Failed(String),
    Ready(Arc<AppState>),
}

pub struct App {
    init: Init,
    prefs: Prefs,
    route: Route,
    shared: Shared,
    toasts: Toasts,
    palette: palette::Palette,
    welcome: welcome::Welcome,
    update: update::UpdateNotice,
    logo: image::Handle,

    dashboard: pages::dashboard::Dashboard,
    models: pages::models::Models,
    chat: pages::chat::Chat,
    benchmark: pages::benchmark::Benchmark,
    mesh: pages::mesh::Mesh,
    account: pages::account::Account,
    settings: pages::settings::Settings,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let prefs = Prefs::load();
        let app = Self {
            init: Init::Starting,
            welcome: welcome::Welcome::new(prefs.onboarded),
            prefs,
            route: Route::Dashboard,
            shared: Shared::default(),
            toasts: Toasts::default(),
            palette: palette::Palette::default(),
            update: update::UpdateNotice::default(),
            logo: image::Handle::from_bytes(LOGO),
            dashboard: Default::default(),
            models: Default::default(),
            chat: Default::default(),
            benchmark: Default::default(),
            mesh: Default::default(),
            account: Default::default(),
            settings: Default::default(),
        };
        // Build the backend off the UI thread. Hardware profiling (GPU
        // enumeration, a disk read test) and opening the registry take
        // seconds on some machines; the window shows "Starting…" meanwhile.
        let boot = Task::perform(init_backend(), Message::Booted);
        (
            app,
            Task::batch([boot, update::check().map(Message::Update)]),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            keyboard::listen().map(Message::Key),
            window::close_requests().map(|_| Message::CloseRequested),
            iced::time::every(update::RECHECK).map(|_| Message::Update(update::Msg::Check)),
        ];
        // One timer, as fast as the most demanding thing on screen needs:
        // the chat's "thinking" dots, a search debounce, toast expiry, or
        // the live mesh status.
        let interval = if self.chat.animating() {
            Some(Duration::from_millis(150))
        } else if self.models.debouncing() || self.mesh.watching() || !self.toasts.is_empty() {
            Some(Duration::from_millis(250))
        } else if matches!(self.route, Route::Dashboard | Route::Mesh) {
            Some(Duration::from_secs(5))
        } else {
            None
        };
        if let Some(every) = interval {
            subs.push(iced::time::every(every).map(|_| Message::Tick));
        }
        Subscription::batch(subs)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let prefs_before = self.prefs.clone();
        let task = self.handle(message);
        if self.prefs != prefs_before {
            self.prefs.save();
        }
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Booted(Ok(state)) => {
                let bootstrap = self.shared.bootstrap(&state);
                let welcome = self.welcome.on_ready(&state);
                self.init = Init::Ready(state);
                Task::batch([bootstrap, self.enter(self.route), welcome])
            }
            Message::Booted(Err(message)) => {
                crate::write_startup_failure(&message);
                self.init = Init::Failed(message);
                Task::none()
            }
            Message::Navigate(route) => self.navigate(route),
            Message::ToggleSidebar => {
                self.prefs.sidebar_collapsed = !self.prefs.sidebar_collapsed;
                Task::none()
            }
            Message::Key(event) => self.on_key(event),
            Message::Tick => {
                self.toasts.expire();
                self.page(|p, env| {
                    Task::batch([
                        p.models.tick(env).map(Message::Models),
                        p.mesh.tick(env).map(Message::Mesh),
                    ])
                })
            }
            Message::DismissToast(id) => {
                self.toasts.dismiss(id);
                Task::none()
            }
            Message::CloseRequested => match &self.init {
                // Deregister from the coordination server rather than leaving
                // peers to time this node out.
                Init::Ready(state) => {
                    let state = Arc::clone(state);
                    Task::perform(
                        async move {
                            let _ = tokio::time::timeout(Duration::from_secs(3), state.stop_mesh())
                                .await;
                        },
                        |()| Message::Exit,
                    )
                }
                _ => Task::done(Message::Exit),
            },
            Message::Exit => {
                crate::telemetry::flush();
                iced::exit()
            }
            Message::Quit => iced::exit(),

            Message::Shared(msg) => {
                let Init::Ready(state) = &self.init else {
                    return Task::none();
                };
                let state = Arc::clone(state);
                let mut nav = None;
                let task = self.shared.update(msg, &state, &mut self.toasts, &mut nav);
                match nav {
                    Some(route) => Task::batch([task, self.navigate(route)]),
                    None => task,
                }
            }
            Message::Welcome(msg) => {
                let ready = match &self.init {
                    Init::Ready(s) => Some(Arc::clone(s)),
                    _ => None,
                };
                let outcome = self.welcome.update(msg, ready.as_ref(), &mut self.shared);
                match outcome {
                    welcome::Outcome::Pending(task) => task,
                    welcome::Outcome::Finished {
                        task,
                        started_install,
                    } => {
                        self.prefs.onboarded = true;
                        if started_install && self.route != Route::Dashboard {
                            // The dashboard is where a download's progress shows.
                            Task::batch([task, self.navigate(Route::Dashboard)])
                        } else {
                            task
                        }
                    }
                }
            }
            Message::Palette(msg) => match self.palette.update(msg) {
                Some(palette::Choice { route, new_chat }) => {
                    if new_chat {
                        self.chat.request_new_chat();
                    }
                    self.navigate(route)
                }
                None => Task::none(),
            },
            Message::Update(msg) => self.update.update(msg).map(Message::Update),

            Message::Dashboard(msg) => {
                self.page(|app, env| app.dashboard.update(msg, env).map(Message::Dashboard))
            }
            Message::Models(msg) => {
                self.page(|app, env| app.models.update(msg, env).map(Message::Models))
            }
            Message::Chat(msg) => {
                self.page(|app, env| app.chat.update(msg, env).map(Message::Chat))
            }
            Message::Benchmark(msg) => {
                self.page(|app, env| app.benchmark.update(msg, env).map(Message::Benchmark))
            }
            Message::Mesh(msg) => {
                self.page(|app, env| app.mesh.update(msg, env).map(Message::Mesh))
            }
            Message::Account(msg) => {
                self.page(|app, env| app.account.update(msg, env).map(Message::Account))
            }
            Message::Settings(msg) => {
                self.page(|app, env| app.settings.update(msg, env).map(Message::Settings))
            }
        }
    }

    /// Run `f` with an [`Env`], then apply the navigation and tasks it asked for.
    fn with_env(
        &mut self,
        f: impl FnOnce(&mut PagesMut, &mut Env) -> Option<Task<Message>>,
    ) -> Option<Task<Message>> {
        let Init::Ready(state) = &self.init else {
            return None;
        };
        let state = Arc::clone(state);
        let mut env = Env {
            state: &state,
            shared: &mut self.shared,
            toasts: &mut self.toasts,
            prefs: &mut self.prefs,
            nav: None,
            tasks: vec![],
        };
        let mut pages = PagesMut {
            dashboard: &mut self.dashboard,
            models: &mut self.models,
            chat: &mut self.chat,
            benchmark: &mut self.benchmark,
            mesh: &mut self.mesh,
            account: &mut self.account,
            settings: &mut self.settings,
        };
        let task = f(&mut pages, &mut env);
        let Env { nav, mut tasks, .. } = env;
        tasks.extend(task);
        if let Some(route) = nav {
            tasks.push(self.navigate(route));
        }
        Some(Task::batch(tasks))
    }

    fn page(&mut self, f: impl FnOnce(&mut PagesMut, &mut Env) -> Task<Message>) -> Task<Message> {
        self.with_env(|pages, env| Some(f(pages, env)))
            .unwrap_or_else(Task::none)
    }

    fn navigate(&mut self, route: Route) -> Task<Message> {
        if route == self.route {
            // Still honour a "new chat" request made while on the chat.
            return self.page(|p, env| p.chat.flush_new_chat(env).map(Message::Chat));
        }
        self.enter(route)
    }

    /// Switch page, running its "on open" refresh.
    fn enter(&mut self, route: Route) -> Task<Message> {
        self.route = route;
        self.page(|p, env| match route {
            Route::Dashboard => p.dashboard.enter(env).map(Message::Dashboard),
            Route::Models => p.models.enter(env).map(Message::Models),
            Route::Chat => p.chat.enter(env).map(Message::Chat),
            Route::Benchmark => p.benchmark.enter(env).map(Message::Benchmark),
            Route::Mesh => Task::none(),
            Route::Account => p.account.enter(env).map(Message::Account),
            Route::Settings => p.settings.enter(env).map(Message::Settings),
        })
    }

    fn on_key(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        if !matches!(self.init, Init::Ready(_)) {
            return Task::none();
        }
        if self.palette.is_open() {
            match key.as_ref() {
                Key::Named(Named::ArrowDown) => {
                    return self.handle(Message::Palette(palette::Msg::Down))
                }
                Key::Named(Named::ArrowUp) => {
                    return self.handle(Message::Palette(palette::Msg::Up))
                }
                Key::Named(Named::Escape) => {
                    return self.handle(Message::Palette(palette::Msg::Close))
                }
                _ => {}
            }
        }
        if !modifiers.command() {
            return Task::none();
        }
        match key.as_ref() {
            Key::Character("k") => self.palette.toggle(),
            Key::Character("n") => {
                self.chat.request_new_chat();
                self.navigate(Route::Chat)
            }
            Key::Character(",") => self.navigate(Route::Settings),
            _ => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = match &self.init {
            Init::Starting => starting_screen(),
            Init::Failed(message) => failed_screen(message),
            Init::Ready(state) => {
                let cx = Cx {
                    state,
                    shared: &self.shared,
                };
                let page: Element<'_, Message> = match self.route {
                    Route::Dashboard => self.dashboard.view(&cx).map(Message::Dashboard),
                    Route::Models => self.models.view(&cx).map(Message::Models),
                    Route::Chat => self.chat.view(&cx).map(Message::Chat),
                    Route::Benchmark => self.benchmark.view(&cx).map(Message::Benchmark),
                    Route::Mesh => self.mesh.view(&cx).map(Message::Mesh),
                    Route::Account => self.account.view(&cx).map(Message::Account),
                    Route::Settings => self.settings.view(&cx).map(Message::Settings),
                };
                row![
                    self.sidebar(),
                    container(page).width(Fill).height(Fill).style(theme::page)
                ]
                .into()
            }
        };

        let mut layers = vec![base];
        if self.palette.is_open() {
            layers.push(modal(
                self.palette.view().map(Message::Palette),
                0.4,
                Some(Message::Palette(palette::Msg::Close)),
                true,
            ));
        }
        if let Some(w) = self.welcome.view(
            match &self.init {
                Init::Ready(s) => Some(&**s),
                _ => None,
            },
            &self.shared,
        ) {
            layers.push(modal(w.map(Message::Welcome), 0.6, None, false));
        }
        let mut corner = column![].spacing(8).align_x(Alignment::End);
        if !self.toasts.is_empty() {
            corner = corner.push(self.toasts.view());
        }
        if let Some(card) = self.update.view() {
            corner = corner.push(card.map(Message::Update));
        }
        layers.push(
            container(corner)
                .padding(16)
                .width(Fill)
                .height(Fill)
                .align_right(Fill)
                .align_bottom(Fill)
                .into(),
        );
        stack(layers).into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let collapsed = self.prefs.sidebar_collapsed;
        let link = |route: Route| -> Element<'_, Message> {
            let active = self.route == route;
            let marker = container(space()).width(3).height(16).style(theme::pill(
                if active {
                    PAW_500
                } else {
                    iced::Color::TRANSPARENT
                },
                2.0,
            ));
            let mut content = row![marker, icon(route.glyph()).size(16)]
                .spacing(12)
                .align_y(Alignment::Center);
            if !collapsed {
                content = content.push(text(route.label()).size(BODY));
            }
            button(content)
                .width(Fill)
                .padding([8, 0])
                .style(theme::button(Kind::Row { selected: active }))
                .on_press(Message::Navigate(route))
                .into()
        };

        let mut header = row![image(self.logo.clone()).width(28).height(28)]
            .spacing(8)
            .align_y(Alignment::Center);
        if !collapsed {
            header = header.push(widgets::strong("HiveBear"));
        }

        let toggle_glyph = if collapsed {
            icon::SIDEBAR_SIMPLE
        } else {
            icon::SIDEBAR
        };
        let mut footer = row![widgets::icon_button(
            toggle_glyph,
            if collapsed {
                "Expand sidebar"
            } else {
                "Collapse sidebar"
            },
            Message::ToggleSidebar
        )]
        .align_y(Alignment::Center);
        if !collapsed {
            footer = footer.push(space::horizontal()).push(widgets::label(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                10.0,
                TEXT_MUTED,
            ));
        }

        let nav = column![
            container(header).padding([12, 6]),
            link(Route::Dashboard),
            link(Route::Models),
            link(Route::Chat),
            rule::horizontal(1),
            link(Route::Benchmark),
            link(Route::Mesh),
            space::vertical(),
            link(Route::Account),
            link(Route::Settings),
            rule::horizontal(1),
            footer,
        ]
        .spacing(4)
        .padding([4, 8]);

        container(nav)
            .width(if collapsed { 56 } else { 224 })
            .height(Fill)
            .style(theme::sidebar)
            .into()
    }
}

/// Mutable access to every page at once, for [`App::with_env`].
struct PagesMut<'a> {
    dashboard: &'a mut pages::dashboard::Dashboard,
    models: &'a mut pages::models::Models,
    chat: &'a mut pages::chat::Chat,
    benchmark: &'a mut pages::benchmark::Benchmark,
    mesh: &'a mut pages::mesh::Mesh,
    account: &'a mut pages::account::Account,
    settings: &'a mut pages::settings::Settings,
}

/// A centred dialog over a dimmed backdrop. `top` pins it to the upper
/// third, like the command palette.
fn modal<'a>(
    content: Element<'a, Message>,
    dim: f32,
    on_backdrop: Option<Message>,
    top: bool,
) -> Element<'a, Message> {
    let mut placed = container(content).width(Fill).height(Fill).center_x(Fill);
    placed = if top {
        placed.padding(iced::Padding {
            top: 140.0,
            ..Default::default()
        })
    } else {
        placed.center_y(Fill)
    };
    let mut area = mouse_area(container(placed).style(theme::backdrop(dim)));
    if let Some(msg) = on_backdrop {
        area = area.on_press(msg);
    }
    opaque(area)
}

fn starting_screen<'a>() -> Element<'a, Message> {
    container(
        column![
            icon(icon::ARROWS_CLOCKWISE).size(22).color(PAW_500),
            widgets::secondary("Starting HiveBear…"),
            widgets::muted("Checking your CPU, memory and GPU."),
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center(Fill)
    .style(theme::page)
    .into()
}

fn failed_screen(message: &str) -> Element<'_, Message> {
    container(
        column![
            icon(icon::WARNING_CIRCLE).size(32).color(DANGER),
            widgets::label("HiveBear could not start", 18.0, TEXT_PRIMARY).font(widgets::BOLD),
            container(widgets::error_banner(message)).max_width(560),
            widgets::muted("Details were written to startup-error.log in HiveBear's data folder."),
            widgets::btn(Kind::Secondary, Some(icon::X), "Quit", Some(Message::Quit)),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    )
    .center(Fill)
    .padding(32)
    .style(theme::page)
    .into()
}

async fn init_backend() -> Result<Arc<AppState>, String> {
    let rt = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let state = AppState::init_with_paths(hivebear_core::AppPaths::new(), rt)?;
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
    })
    .await
    .unwrap_or_else(|e| Err(format!("Initialisation stopped unexpectedly: {e}")))
}

/// A vertically scrolling, padded page body (the web app's `Surface`).
pub fn scroll_page<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    scrollable(container(content).padding(24).width(Fill))
        .height(Fill)
        .width(Length::Fill)
        .into()
}
