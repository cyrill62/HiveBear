use std::time::{Duration, Instant};

use iced::widget::{column, container, row, rule, scrollable};
use iced::{Alignment, Element, Fill, Task};

use hivebear_registry::metadata::SearchResult;
use hivebear_registry::ModelMetadata;

use crate::backend;
use crate::error::CmdResult;
use crate::ui::icon;
use crate::ui::theme::{self, *};
use crate::ui::util::{format_bytes, thousands};
use crate::ui::widgets::{self, Kind, Tone};
use crate::ui::{call, Cx, Env};

const DEBOUNCE: Duration = Duration::from_millis(300);
const SUGGESTIONS: &[&str] = &["llama", "mistral", "phi", "codestral", "qwen"];

#[derive(Debug, Clone)]
pub enum Msg {
    Tab(usize),
    Query(String),
    Search,
    Suggest(&'static str),
    Searched(CmdResult<Vec<SearchResult>>),
    Install(String),
    Remove(String),
    Removed(String, CmdResult<u64>),
}

#[derive(Default)]
pub struct Models {
    tab: usize,
    query: String,
    /// When the query last changed, for search-as-you-type.
    edited_at: Option<Instant>,
    results: Vec<SearchResult>,
    searched: bool,
    searching: bool,
    removing: Option<String>,
}

impl Models {
    pub fn enter(&mut self, env: &mut Env) -> Task<Msg> {
        let t = env.shared.refresh_installed(env.state);
        env.spawn(t);
        Task::none()
    }

    pub fn debouncing(&self) -> bool {
        self.edited_at.is_some()
    }

    /// Timer tick: fire a debounced search once typing pauses.
    pub fn tick(&mut self, env: &mut Env) -> Task<Msg> {
        match self.edited_at {
            Some(at) if at.elapsed() >= DEBOUNCE => self.search(env),
            _ => Task::none(),
        }
    }

    fn search(&mut self, env: &mut Env) -> Task<Msg> {
        self.edited_at = None;
        let query = self.query.trim().to_string();
        if query.is_empty() {
            self.results.clear();
            self.searched = false;
            return Task::none();
        }
        self.searching = true;
        call(
            env.state,
            |s| async move { backend::registry::search_models(&s, &query).await },
            Msg::Searched,
        )
    }

    pub fn update(&mut self, msg: Msg, env: &mut Env) -> Task<Msg> {
        match msg {
            Msg::Tab(t) => self.tab = t,
            Msg::Query(q) => {
                self.edited_at = (q.trim().chars().count() >= 2).then(Instant::now);
                self.query = q;
            }
            Msg::Search => return self.search(env),
            Msg::Suggest(q) => {
                self.query = q.to_string();
                return self.search(env);
            }
            Msg::Searched(result) => {
                self.searching = false;
                self.searched = true;
                match result {
                    Ok(r) => self.results = r,
                    Err(e) => env.toasts.error(format!("Search failed: {e}")),
                }
            }
            Msg::Install(id) => {
                let t = env.shared.install(env.state, &id, None);
                env.spawn(t);
            }
            Msg::Remove(id) if self.removing.is_none() => {
                self.removing = Some(id.clone());
                let model = id.clone();
                return call(
                    env.state,
                    |s| async move { backend::registry::remove_model(&s, &model).await },
                    move |r| Msg::Removed(id, r),
                );
            }
            Msg::Remove(_) => {}
            Msg::Removed(id, result) => {
                self.removing = None;
                match result {
                    Ok(_) => {
                        env.toasts.success(format!("Removed {id}."));
                        for r in self.results.iter_mut().filter(|r| r.metadata.id == id) {
                            r.is_installed = false;
                            r.metadata.installed = None;
                        }
                        let t = env.shared.refresh_installed(env.state);
                        env.spawn(t);
                    }
                    Err(e) => env.toasts.error(format!("Could not remove {id}: {e}")),
                }
            }
        }
        Task::none()
    }

    pub fn view<'a>(&'a self, cx: &Cx<'a>) -> Element<'a, Msg> {
        let shared = cx.shared;
        let mut header = column![
            widgets::page_title("Models"),
            widgets::segmented(
                vec![
                    "Search".into(),
                    format!("Installed ({})", shared.installed.len())
                ],
                self.tab,
                Msg::Tab
            ),
        ]
        .spacing(12);
        if self.tab == 0 {
            header = header.push(
                row![
                    widgets::input(
                        "Search models (e.g. llama, codestral, phi)...",
                        &self.query,
                        Msg::Query
                    )
                    .icon(iced::widget::text_input::Icon {
                        font: icon::FONT,
                        code_point: icon::MAGNIFYING_GLASS.chars().next().unwrap_or(' '),
                        size: Some(14.0.into()),
                        spacing: 8.0,
                        side: iced::widget::text_input::Side::Left,
                    })
                    .on_submit(Msg::Search),
                    widgets::btn(Kind::Primary, None, "Search", Some(Msg::Search)),
                ]
                .spacing(8),
            );
        }

        let mut top = column![
            container(header).padding([24, 24]).width(Fill),
            rule::horizontal(1)
        ];
        if let Some(job) = &shared.install {
            top = top.push(
                container(widgets::download_progress(
                    &job.model_id,
                    job.progress.as_ref(),
                ))
                .padding([8, 24]),
            );
        }
        if let Some(e) = &shared.install_error {
            top = top.push(container(widgets::error_banner(e.as_str())).padding([8, 24]));
        }

        let installing = shared.install.as_ref().map(|j| j.model_id.as_str());
        let removing = self.removing.as_deref();
        let is_installed = |id: &str| shared.installed.iter().any(|m| m.id == id);

        let mut list = column![].spacing(8);
        if self.tab == 0 {
            if self.searching {
                list = list.push(widgets::muted("Searching..."));
            } else if self.results.is_empty() {
                let (title, desc) = if self.searched {
                    ("No results found", "Try a different search term.")
                } else {
                    (
                        "Search for models",
                        "Discover models from HuggingFace and install them locally.",
                    )
                };
                let actions = (!self.searched).then(|| {
                    let mut chips = row![].spacing(8);
                    for q in SUGGESTIONS {
                        chips = chips.push(widgets::chip(q, Msg::Suggest(q)));
                    }
                    Element::from(chips)
                });
                list = list.push(widgets::empty_state(
                    icon::MAGNIFYING_GLASS,
                    title,
                    desc,
                    actions,
                ));
            } else {
                for r in &self.results {
                    let installed = r.is_installed || is_installed(&r.metadata.id);
                    list = list.push(model_card(
                        &r.metadata,
                        installed,
                        r.compatibility_score,
                        installing,
                        removing,
                    ));
                }
            }
        } else if shared.installed.is_empty() {
            list = list.push(widgets::empty_state(
                icon::PACKAGE,
                "No models installed",
                "Search and install a model to get started.",
                Some(widgets::btn(Kind::Primary, None, "Browse Models", Some(Msg::Tab(0))).into()),
            ));
        } else {
            for m in &shared.installed {
                list = list.push(model_card(m, true, None, installing, removing));
            }
        }

        column![
            top,
            scrollable(container(list).padding(24).width(Fill)).height(Fill)
        ]
        .into()
    }
}

fn model_card<'a>(
    m: &'a ModelMetadata,
    is_installed: bool,
    compatibility: Option<f64>,
    installing: Option<&str>,
    removing: Option<&str>,
) -> Element<'a, Msg> {
    let this_installing = installing == Some(m.id.as_str());
    let this_removing = removing == Some(m.id.as_str());

    let action = if is_installed {
        widgets::btn_sm(
            Kind::Secondary,
            Some(if this_removing {
                icon::ARROWS_CLOCKWISE
            } else {
                icon::TRASH
            }),
            "Remove",
            (!this_removing).then(|| Msg::Remove(m.id.clone())),
        )
    } else {
        // One download at a time.
        widgets::btn_sm(
            Kind::Primary,
            Some(if this_installing {
                icon::ARROWS_CLOCKWISE
            } else {
                icon::DOWNLOAD_SIMPLE
            }),
            "Install",
            installing.is_none().then(|| Msg::Install(m.id.clone())),
        )
    };

    let mut info = column![
        widgets::strong(m.name.as_str()),
        row![
            widgets::muted(format!("{}B params", m.params_billions)),
            widgets::muted("·"),
            widgets::badge(m.category.to_string(), Tone::Default),
            widgets::muted("·"),
            widgets::muted(format!("ctx {}", thousands(m.context_length.into()))),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    ]
    .spacing(4)
    .width(Fill);
    if let Some(desc) = &m.description {
        let desc: String = desc.chars().take(220).collect();
        info = info.push(widgets::label(desc, SMALL, TEXT_SECONDARY));
    }

    let mut meta = row![].spacing(10).align_y(Alignment::Center);
    if let Some(score) = compatibility {
        let tone = if score >= 0.7 {
            Tone::Success
        } else if score >= 0.4 {
            Tone::Warning
        } else {
            Tone::Danger
        };
        meta = meta.push(widgets::badge(
            format!("{:.0}% compatible", score * 100.0),
            tone,
        ));
    }
    if let Some(inst) = &m.installed {
        meta = meta.push(widgets::badge(
            format!("✓ {}", format_bytes(inst.size_bytes)),
            Tone::Success,
        ));
    }
    if let Some(n) = m.downloads_count {
        meta = meta.push(widgets::muted(format!("{} downloads", thousands(n))));
    }
    if !m.tags.is_empty() {
        let tags: Vec<&str> = m.tags.iter().take(3).map(String::as_str).collect();
        meta = meta.push(widgets::muted(tags.join(" · ")));
    }

    let body = column![row![info, action].spacing(12), meta].spacing(10);
    // Installed models carry the accent, as the web app's left border did.
    let border_color = if this_installing || is_installed {
        tint(PAW_500, 0.45)
    } else {
        BORDER
    };
    container(body)
        .padding(16)
        .width(Fill)
        .style(move |t| iced::widget::container::Style {
            border: iced::Border {
                color: border_color,
                ..theme::card(t).border
            },
            ..theme::card(t)
        })
        .into()
}
