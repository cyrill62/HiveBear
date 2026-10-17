//! One entity per page, kept alive across navigation so a running chat or
//! benchmark keeps its state while you look elsewhere.

mod account;
mod benchmark;
pub mod chat;
mod dashboard;
mod mesh;
mod models;
mod settings;

use std::sync::Arc;

use gpui_kit::*;

use super::shared::Shared;
use super::{Navigate, Route, Shell};
use crate::state::AppState;

pub struct Pages {
    pub shared: Entity<Shared>,
    dashboard: Entity<dashboard::Dashboard>,
    models: Entity<models::Models>,
    pub chat: Entity<chat::Chat>,
    benchmark: Entity<benchmark::Benchmark>,
    mesh: Entity<mesh::Mesh>,
    account: Entity<account::Account>,
    settings: Entity<settings::Settings>,
}

/// Forward a page's navigation requests to the shell.
fn forward<T: EventEmitter<Navigate>>(
    page: &Entity<T>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> Subscription {
    cx.subscribe_in(page, window, |shell, _, nav: &Navigate, window, cx| {
        shell.navigate(nav.0, window, cx);
    })
}

impl Pages {
    pub fn new(
        state: &Arc<AppState>,
        shared: &Entity<Shared>,
        window: &mut Window,
        cx: &mut Context<Shell>,
    ) -> (Self, Vec<Subscription>) {
        let s = || Arc::clone(state);
        let sh = || shared.clone();
        let pages = Self {
            shared: shared.clone(),
            dashboard: cx.new(|cx| dashboard::Dashboard::new(s(), sh(), window, cx)),
            models: cx.new(|cx| models::Models::new(s(), sh(), window, cx)),
            chat: cx.new(|cx| chat::Chat::new(s(), sh(), window, cx)),
            benchmark: cx.new(|cx| benchmark::Benchmark::new(s(), sh(), window, cx)),
            mesh: cx.new(|cx| mesh::Mesh::new(s(), sh(), window, cx)),
            account: cx.new(|cx| account::Account::new(s(), sh(), window, cx)),
            settings: cx.new(|cx| settings::Settings::new(s(), sh(), window, cx)),
        };
        let subscriptions = vec![
            forward(&pages.dashboard, window, cx),
            forward(&pages.models, window, cx),
            forward(&pages.chat, window, cx),
            forward(&pages.benchmark, window, cx),
            forward(&pages.mesh, window, cx),
            forward(&pages.account, window, cx),
            forward(&pages.settings, window, cx),
            forward(shared, window, cx),
        ];
        (pages, subscriptions)
    }

    /// Called when a page is navigated to — the web app's "on mount" fetches.
    pub fn enter(&self, route: Route, window: &mut Window, cx: &mut App) {
        match route {
            Route::Dashboard => self.dashboard.update(cx, |p, cx| p.enter(window, cx)),
            Route::Models => self.models.update(cx, |p, cx| p.enter(window, cx)),
            Route::Chat => self.chat.update(cx, |p, cx| p.enter(window, cx)),
            Route::Benchmark => self.benchmark.update(cx, |p, cx| p.enter(window, cx)),
            Route::Mesh => self.mesh.update(cx, |p, cx| p.enter(window, cx)),
            Route::Account => self.account.update(cx, |p, cx| p.enter(window, cx)),
            Route::Settings => self.settings.update(cx, |p, cx| p.enter(window, cx)),
        }
    }

    pub fn view(&self, route: Route) -> AnyView {
        match route {
            Route::Dashboard => self.dashboard.clone().into(),
            Route::Models => self.models.clone().into(),
            Route::Chat => self.chat.clone().into(),
            Route::Benchmark => self.benchmark.clone().into(),
            Route::Mesh => self.mesh.clone().into(),
            Route::Account => self.account.clone().into(),
            Route::Settings => self.settings.clone().into(),
        }
    }
}
