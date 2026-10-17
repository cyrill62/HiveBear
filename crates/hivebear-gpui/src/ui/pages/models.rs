use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use hivebear_registry::metadata::SearchResult;
use hivebear_registry::ModelMetadata;

use crate::backend;
use crate::state::AppState;
use crate::ui::shared::Shared;
use crate::ui::theme::*;
use crate::ui::util::{format_bytes, thousands};
use crate::ui::widgets::{self, Tone};
use crate::ui::{cmd, rt, toast_err, toast_ok, Navigate};

const DEBOUNCE: Duration = Duration::from_millis(300);
const SUGGESTIONS: &[&str] = &["llama", "mistral", "phi", "codestral", "qwen"];

pub struct Models {
    state: Arc<AppState>,
    shared: Entity<Shared>,
    query: Entity<InputState>,
    installed_tab: bool,
    results: Vec<SearchResult>,
    searched: bool,
    searching: bool,
    removing: Option<String>,
    /// The pending debounced or running search; replacing it cancels it.
    search_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Navigate> for Models {}

impl Models {
    pub fn new(
        state: Arc<AppState>,
        shared: Entity<Shared>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search models (e.g. llama, codestral, phi)...")
        });
        let _subscriptions = vec![
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &query,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    // Debounced search-as-you-type.
                    InputEvent::Change => {
                        let q = this.query.read(cx).value().to_string();
                        if q.trim().chars().count() >= 2 {
                            this.search_task = Some(cx.spawn(async move |this, cx| {
                                cx.background_executor().timer(DEBOUNCE).await;
                                this.update(cx, |this, cx| this.search(q, cx)).ok();
                            }));
                        }
                    }
                    InputEvent::PressEnter { .. } => {
                        let q = this.query.read(cx).value().to_string();
                        this.search(q, cx);
                    }
                    _ => {}
                },
            ),
        ];
        Self {
            state,
            shared,
            query,
            installed_tab: false,
            results: vec![],
            searched: false,
            searching: false,
            removing: None,
            search_task: None,
            _subscriptions,
        }
    }

    pub fn enter(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.shared.update(cx, |s, cx| s.refresh_installed(cx));
    }

    fn search(&mut self, query: String, cx: &mut Context<Self>) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.results.clear();
            self.searched = false;
            self.search_task = None;
            cx.notify();
            return;
        }
        self.searching = true;
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move {
                backend::registry::search_models(&s, &query).await
            })
            .await;
            this.update(cx, |this, cx| {
                this.searching = false;
                this.searched = true;
                match result {
                    Ok(r) => this.results = r,
                    Err(e) => toast_err(cx, format!("Search failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn suggest(&mut self, query: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.query
            .update(cx, |q, cx| q.set_value(query, window, cx));
        self.search(query.to_string(), cx);
    }

    fn remove(&mut self, id: String, cx: &mut Context<Self>) {
        if self.removing.is_some() {
            return;
        }
        self.removing = Some(id.clone());
        cx.notify();
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let model = id.clone();
            let result = cmd(rt, async move {
                backend::registry::remove_model(&s, &model).await
            })
            .await;
            this.update(cx, |this, cx| {
                this.removing = None;
                match result {
                    Ok(_) => {
                        toast_ok(cx, format!("Removed {id}."));
                        for r in this.results.iter_mut().filter(|r| r.metadata.id == id) {
                            r.is_installed = false;
                            r.metadata.installed = None;
                        }
                        this.shared.update(cx, |s, cx| s.refresh_installed(cx));
                    }
                    Err(e) => toast_err(cx, format!("Could not remove {id}: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn model_card(
        &self,
        m: &ModelMetadata,
        is_installed: bool,
        compatibility: Option<f64>,
        cx: &mut Context<Self>,
    ) -> Div {
        let installing = self
            .shared
            .read(cx)
            .install
            .as_ref()
            .map(|j| j.model_id.clone());
        let this_installing = installing.as_deref() == Some(m.id.as_str());
        let this_removing = self.removing.as_deref() == Some(m.id.as_str());
        let id = m.id.clone();

        let action = if is_installed {
            widgets::button(SharedString::from(format!("remove-{}", m.id)), "Remove")
                .icon(IconName::Trash)
                .outline()
                .small()
                .loading(this_removing)
                .on_click(cx.listener(move |this, _, _, cx| this.remove(id.clone(), cx)))
        } else {
            // One download at a time.
            widgets::button(SharedString::from(format!("install-{}", m.id)), "Install")
                .icon(IconName::Download)
                .primary()
                .small()
                .loading(this_installing)
                .disabled(installing.is_some())
                .on_click(cx.listener(move |this, _, _, cx| {
                    let id = id.clone();
                    this.shared.update(cx, |s, cx| s.install(&id, None, cx));
                }))
        };

        let mut meta = h_flex().gap_3().text_xs().text_color(c(TEXT_MUTED));
        if let Some(score) = compatibility {
            let tone = if score >= 0.7 {
                Tone::Success
            } else if score >= 0.4 {
                Tone::Warning
            } else {
                Tone::Danger
            };
            meta = meta.child(widgets::badge(
                format!("{:.0}% compatible", score * 100.0),
                tone,
            ));
        }
        if let Some(inst) = &m.installed {
            meta = meta.child(widgets::badge(
                format!("✓ {}", format_bytes(inst.size_bytes)),
                Tone::Success,
            ));
        }
        if let Some(n) = m.downloads_count {
            meta = meta.child(format!("{} downloads", thousands(n)));
        }
        if !m.tags.is_empty() {
            let tags: Vec<&str> = m.tags.iter().take(3).map(String::as_str).collect();
            meta = meta.child(tags.join(" · "));
        }

        // Installed models carry the accent, as the web app's left border did.
        widgets::card()
            .when(is_installed || this_installing, |d| {
                d.border_color(ca(PAW_500, 0.45))
            })
            .child(
                h_flex()
                    .items_start()
                    .gap_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(m.name.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .text_xs()
                                    .text_color(c(TEXT_MUTED))
                                    .child(format!("{}B params", m.params_billions))
                                    .child("·")
                                    .child(widgets::badge(m.category.to_string(), Tone::Default))
                                    .child("·")
                                    .child(format!("ctx {}", thousands(m.context_length.into()))),
                            )
                            .children(m.description.as_ref().map(|d| {
                                let d: String = d.chars().take(220).collect();
                                div().text_xs().text_color(c(TEXT_SECONDARY)).child(d)
                            })),
                    )
                    .child(action),
            )
            .child(meta.mt_3())
    }
}

impl Render for Models {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (installed, install_job, install_error) = {
            let s = self.shared.read(cx);
            (
                s.installed.clone(),
                s.install
                    .as_ref()
                    .map(|j| (j.model_id.clone(), j.progress.clone())),
                s.install_error.clone(),
            )
        };
        let installed_ids: Vec<String> = installed.iter().map(|m| m.id.clone()).collect();

        let tab = |id: &'static str, label: String, on: bool, cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex_1()
                .py_1p5()
                .rounded(px(6.))
                .text_center()
                .text_sm()
                .cursor_pointer()
                .text_color(c(if on { TEXT_PRIMARY } else { TEXT_MUTED }))
                .when(on, |d| {
                    d.bg(c(SURFACE_RAISED)).font_weight(FontWeight::MEDIUM)
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.installed_tab = id == "tab-installed";
                    cx.notify();
                }))
        };
        let tabs = h_flex()
            .p_0p5()
            .gap_0p5()
            .rounded(px(8.))
            .bg(c(SURFACE_OVERLAY))
            .child(tab("tab-search", "Search".into(), !self.installed_tab, cx))
            .child(tab(
                "tab-installed",
                format!("Installed ({})", installed.len()),
                self.installed_tab,
                cx,
            ));

        let mut header = v_flex()
            .gap_4()
            .p_6()
            .pb_4()
            .child(widgets::page_title("Models"))
            .child(tabs);
        if !self.installed_tab {
            header =
                header.child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&self.query).prefix(
                            gpui_kit::component::Icon::new(IconName::Search).size(px(14.)),
                        )))
                        .child(widgets::button("search", "Search").primary().on_click(
                            cx.listener(|this, _, _, cx| {
                                let q = this.query.read(cx).value().to_string();
                                this.search(q, cx);
                            }),
                        )),
                );
        }

        let mut list = v_flex().gap_3();
        if !self.installed_tab {
            if self.searching {
                list = list.child(widgets::muted("Searching..."));
            } else if self.results.is_empty() {
                let (title, desc) = if self.searched {
                    ("No results found", "Try a different search term.")
                } else {
                    (
                        "Search for models",
                        "Discover models from HuggingFace and install them locally.",
                    )
                };
                let mut empty = widgets::empty_state(IconName::Search, title, desc);
                if !self.searched {
                    empty = empty.child(h_flex().pt_1().gap_2().children(SUGGESTIONS.iter().map(
                        |q| {
                            widgets::chip(*q, *q).on_click(
                                cx.listener(move |this, _, window, cx| this.suggest(q, window, cx)),
                            )
                        },
                    )));
                }
                list = list.child(empty);
            } else {
                let results = self.results.clone();
                for r in &results {
                    let is_installed = r.is_installed || installed_ids.contains(&r.metadata.id);
                    list = list.child(self.model_card(
                        &r.metadata,
                        is_installed,
                        r.compatibility_score,
                        cx,
                    ));
                }
            }
        } else if installed.is_empty() {
            list = list.child(
                widgets::empty_state(
                    IconName::Package,
                    "No models installed",
                    "Search and install a model to get started.",
                )
                .child(
                    widgets::button("browse", "Browse Models")
                        .primary()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.installed_tab = false;
                            cx.notify();
                        })),
                ),
            );
        } else {
            for m in &installed {
                list = list.child(self.model_card(m, true, None, cx));
            }
        }

        v_flex()
            .size_full()
            .child(header)
            .child(div().h(px(1.)).bg(c(BORDER)))
            .children(install_job.map(|(id, progress)| {
                div()
                    .px_6()
                    .pt_4()
                    .child(widgets::download_progress(&id, progress.as_ref()))
            }))
            .children(install_error.map(|e| div().px_6().pt_4().child(widgets::banner(DANGER, e))))
            .child(
                div()
                    .id("models-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(div().p_6().child(list)),
            )
    }
}
