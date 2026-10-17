mod account;
mod benchmark;
pub mod chat;
mod dashboard;
mod mesh;
mod models;
mod settings;

use eframe::egui::{self, Margin, Ui};

use super::{Env, Route};

/// Every page, kept alive across navigation so a running chat or benchmark
/// keeps its state while you look elsewhere.
#[derive(Default)]
pub struct Pages {
    dashboard: dashboard::Dashboard,
    models: models::Models,
    pub chat: chat::Chat,
    benchmark: benchmark::Benchmark,
    mesh: mesh::Mesh,
    account: account::Account,
    settings: settings::Settings,
}

impl Pages {
    /// Called when a page is navigated to — the web app's "on mount" fetches.
    pub fn enter(&mut self, route: Route, env: &mut Env) {
        match route {
            Route::Dashboard => self.dashboard.enter(env),
            Route::Models => self.models.enter(env),
            Route::Chat => self.chat.enter(env),
            Route::Benchmark => self.benchmark.enter(env),
            Route::Mesh => {}
            Route::Account => self.account.enter(env),
            Route::Settings => self.settings.enter(env),
        }
    }

    pub fn show(&mut self, route: Route, ui: &mut Ui, env: &mut Env) {
        // Pages with background work collect it even while hidden.
        self.chat.poll(env);
        self.benchmark.poll(env);
        self.account.poll(env);
        self.settings.poll(env);
        self.models.poll(env);

        match route {
            Route::Dashboard => scroll_page(ui, "dashboard", |ui| self.dashboard.show(ui, env)),
            Route::Models => self.models.show(ui, env),
            Route::Chat => self.chat.show(ui, env),
            Route::Benchmark => scroll_page(ui, "benchmark", |ui| self.benchmark.show(ui, env)),
            Route::Mesh => scroll_page(ui, "mesh", |ui| self.mesh.show(ui, env)),
            Route::Account => scroll_page(ui, "account", |ui| self.account.show(ui, env)),
            Route::Settings => self.settings.show(ui, env),
        }
    }
}

/// A padded, vertically scrolling page body (the web app's `Surface`).
pub fn scroll_page(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 12.0;
                    add(ui);
                });
        });
}
