use std::time::{Duration, Instant};

use eframe::egui::{self, Align, CornerRadius, Frame, Layout, Margin, Stroke, Ui};
use egui_phosphor::regular as icon;
use hivebear_registry::metadata::SearchResult;
use hivebear_registry::ModelMetadata;

use crate::backend;
use crate::error::CmdResult;
use crate::tasks::{poll_slot, Task};
use crate::ui::theme::*;
use crate::ui::util::{format_bytes, thousands};
use crate::ui::widgets::{self, Btn, Tone};
use crate::ui::Env;

const DEBOUNCE: Duration = Duration::from_millis(300);
const SUGGESTIONS: &[&str] = &["llama", "mistral", "phi", "codestral", "qwen"];

#[derive(Default)]
pub struct Models {
    tab: usize,
    query: String,
    /// When the query last changed, for search-as-you-type.
    edited_at: Option<Instant>,
    results: Vec<SearchResult>,
    searched: bool,
    search: Option<Task<CmdResult<Vec<SearchResult>>>>,
    search_error: Option<String>,
    removing: Option<(String, Task<CmdResult<u64>>)>,
}

enum Action {
    Install(String),
    Remove(String),
}

impl Models {
    pub fn enter(&mut self, env: &mut Env) {
        env.shared.refresh_installed(env.ctx);
    }

    fn start_search(&mut self, env: &Env, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.results.clear();
            self.searched = false;
            return;
        }
        self.edited_at = None;
        self.search_error = None;
        self.search = Some(
            env.ctx
                .run(move |s| async move { backend::registry::search_models(&s, &query).await }),
        );
    }

    pub fn poll(&mut self, env: &mut Env) {
        if let Some(result) = poll_slot(&mut self.search) {
            self.searched = true;
            match result {
                Ok(r) => self.results = r,
                Err(e) => {
                    env.toasts.error(format!("Search failed: {e}"));
                    self.search_error = Some(e);
                }
            }
        }
        if let Some((id, task)) = &mut self.removing {
            if let Some(result) = task.poll() {
                let id = id.clone();
                self.removing = None;
                match result {
                    Ok(Ok(_)) => {
                        env.toasts.success(format!("Removed {id}."));
                        env.shared.refresh_installed(env.ctx);
                        for r in self.results.iter_mut().filter(|r| r.metadata.id == id) {
                            r.is_installed = false;
                            r.metadata.installed = None;
                        }
                    }
                    Ok(Err(e)) => env.toasts.error(format!("Could not remove {id}: {e}")),
                    Err(()) => {}
                }
            }
        }
        // A model finished installing: mark it in the search results too.
        let installed: Vec<&str> = env.shared.installed.iter().map(|m| m.id.as_str()).collect();
        for r in &mut self.results {
            if !r.is_installed && installed.contains(&r.metadata.id.as_str()) {
                r.is_installed = true;
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut Env) {
        // Debounced search-as-you-type.
        if let Some(at) = self.edited_at {
            let waited = at.elapsed();
            if waited >= DEBOUNCE {
                let q = self.query.clone();
                self.start_search(env, &q);
            } else {
                ui.ctx().request_repaint_after(DEBOUNCE - waited);
            }
        }

        let mut action = None;

        Frame::new()
            .inner_margin(Margin {
                left: 24,
                right: 24,
                top: 24,
                bottom: 16,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 12.0;
                widgets::page_title(ui, "Models");
                let installed_label = format!("Installed ({})", env.shared.installed.len());
                if let Some(tab) = widgets::segmented(ui, &["Search", &installed_label], self.tab) {
                    self.tab = tab;
                }
                if self.tab == 0 {
                    ui.horizontal(|ui| {
                        let edit = widgets::search_field(
                            ui,
                            &mut self.query,
                            "Search models (e.g. llama, codestral, phi)...",
                            ui.available_width() - 96.0,
                        );
                        if edit.changed() {
                            self.edited_at =
                                (self.query.trim().chars().count() >= 2).then(Instant::now);
                        }
                        let submitted =
                            edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if Btn::primary("", "Search").show(ui).clicked() || submitted {
                            let q = self.query.clone();
                            self.start_search(env, &q);
                        }
                    });
                }
            });
        ui.separator();

        if let Some(job) = &env.shared.install {
            Frame::new()
                .inner_margin(Margin::symmetric(24, 8))
                .show(ui, |ui| {
                    widgets::download_progress(ui, &job.model_id, job.progress().as_ref());
                });
        }
        if let Some(error) = env.shared.install_error.clone() {
            Frame::new()
                .inner_margin(Margin::symmetric(24, 8))
                .show(ui, |ui| {
                    widgets::error_banner(ui, &error);
                });
        }

        let installing = env.shared.installing().map(str::to_string);
        let removing = self.removing.as_ref().map(|(id, _)| id.clone());

        egui::ScrollArea::vertical()
            .id_salt("models-results")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if self.tab == 0 {
                        if self.search.is_some() {
                            ui.horizontal(|ui| {
                                widgets::spinner(ui);
                                ui.label(widgets::muted("Searching..."));
                            });
                        } else if self.results.is_empty() {
                            let (title, desc) = if self.searched {
                                ("No results found", "Try a different search term.")
                            } else {
                                (
                                    "Search for models",
                                    "Discover models from HuggingFace and install them locally.",
                                )
                            };
                            let mut picked = None;
                            widgets::empty_state(ui, icon::MAGNIFYING_GLASS, title, desc, |ui| {
                                if !self.searched {
                                    ui.horizontal(|ui| {
                                        let w = SUGGESTIONS.len() as f32 * 72.0;
                                        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                                        for q in SUGGESTIONS {
                                            if widgets::chip(ui, q).clicked() {
                                                picked = Some(q.to_string());
                                            }
                                        }
                                    });
                                }
                            });
                            if let Some(q) = picked {
                                self.query = q.clone();
                                self.start_search(env, &q);
                            }
                        } else {
                            for r in &self.results {
                                if let Some(a) = model_card(
                                    ui,
                                    &r.metadata,
                                    r.is_installed,
                                    r.compatibility_score,
                                    installing.as_deref(),
                                    removing.as_deref(),
                                ) {
                                    action = Some(a);
                                }
                                ui.add_space(4.0);
                            }
                        }
                    } else if env.shared.installed.is_empty() {
                        let mut browse = false;
                        widgets::empty_state(
                            ui,
                            icon::PACKAGE,
                            "No models installed",
                            "Search and install a model to get started.",
                            |ui| {
                                browse = Btn::primary("", "Browse Models").show(ui).clicked();
                            },
                        );
                        if browse {
                            self.tab = 0;
                        }
                    } else {
                        for m in &env.shared.installed {
                            if let Some(a) = model_card(
                                ui,
                                m,
                                true,
                                None,
                                installing.as_deref(),
                                removing.as_deref(),
                            ) {
                                action = Some(a);
                            }
                            ui.add_space(4.0);
                        }
                    }
                });
            });

        match action {
            Some(Action::Install(id)) => env.shared.install(env.ctx, &id, None),
            Some(Action::Remove(id)) if self.removing.is_none() => {
                let model = id.clone();
                let task = env
                    .ctx
                    .run(move |s| async move { backend::registry::remove_model(&s, &model).await });
                self.removing = Some((id, task));
            }
            _ => {}
        }
    }
}

fn model_card(
    ui: &mut Ui,
    m: &ModelMetadata,
    is_installed: bool,
    compatibility: Option<f64>,
    installing: Option<&str>,
    removing: Option<&str>,
) -> Option<Action> {
    let this_installing = installing == Some(m.id.as_str());
    let this_removing = removing == Some(m.id.as_str());
    let busy = this_installing || this_removing;
    let mut action = None;

    let stroke = if this_installing {
        Stroke::new(1.0, tint(PAW_500, 0.4))
    } else {
        Stroke::new(1.0, BORDER)
    };
    let response = Frame::new()
        .fill(SURFACE_RAISED)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - 110.0);
                    ui.add(egui::Label::new(widgets::text(&m.name).strong()).truncate());
                    ui.horizontal(|ui| {
                        ui.label(widgets::muted(format!("{}B params", m.params_billions)));
                        ui.label(widgets::muted("·"));
                        widgets::badge(ui, &m.category.to_string(), Tone::Default);
                        ui.label(widgets::muted("·"));
                        ui.label(widgets::muted(format!(
                            "ctx {}",
                            thousands(m.context_length.into())
                        )));
                    });
                    if let Some(desc) = &m.description {
                        let desc: String = desc.chars().take(220).collect();
                        ui.add(egui::Label::new(widgets::secondary(desc).size(SMALL)).wrap());
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if is_installed {
                        if Btn::secondary(icon::TRASH, "Remove")
                            .small()
                            .enabled(!busy)
                            .busy(this_removing)
                            .show(ui)
                            .clicked()
                        {
                            action = Some(Action::Remove(m.id.clone()));
                        }
                    } else {
                        let others_busy = installing.is_some() && !this_installing;
                        if Btn::primary(icon::DOWNLOAD_SIMPLE, "Install")
                            .small()
                            .enabled(!others_busy)
                            .busy(this_installing)
                            .tooltip(if others_busy {
                                "Another download is in progress"
                            } else {
                                "Download and install"
                            })
                            .show(ui)
                            .clicked()
                        {
                            action = Some(Action::Install(m.id.clone()));
                        }
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                if let Some(score) = compatibility {
                    let tone = if score >= 0.7 {
                        Tone::Success
                    } else if score >= 0.4 {
                        Tone::Warning
                    } else {
                        Tone::Danger
                    };
                    widgets::badge(ui, &format!("{:.0}% compatible", score * 100.0), tone);
                }
                if let Some(inst) = &m.installed {
                    widgets::badge(
                        ui,
                        &format!("{} {}", icon::CHECK, format_bytes(inst.size_bytes)),
                        Tone::Success,
                    );
                }
                if let Some(n) = m.downloads_count {
                    ui.label(widgets::muted(format!("{} downloads", thousands(n))));
                }
                if !m.tags.is_empty() {
                    let tags: Vec<&str> = m.tags.iter().take(3).map(String::as_str).collect();
                    ui.label(widgets::muted(tags.join(" · ")));
                }
            });
        })
        .response;

    if is_installed {
        // The web app's left accent border on installed models.
        let r = response.rect;
        let bar = egui::Rect::from_min_max(r.left_top(), egui::pos2(r.left() + 3.0, r.bottom()));
        ui.painter().rect_filled(
            bar,
            CornerRadius {
                nw: RADIUS_LG,
                sw: RADIUS_LG,
                ne: 0,
                se: 0,
            },
            PAW_500,
        );
    }
    action
}
