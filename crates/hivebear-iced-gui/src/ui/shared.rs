//! State more than one page reads: what is installed, what is loaded, the
//! running download. It lives once, above the pages, so a download keeps its
//! progress while you look elsewhere.

use std::sync::{Arc, Mutex};

use iced::futures::SinkExt;
use iced::Task;

use hivebear_core::ModelRecommendation;
use hivebear_inference::types::ModelInfo;
use hivebear_registry::download::DownloadProgress;
use hivebear_registry::{InstalledInfo, ModelMetadata};

use super::toast::Toasts;
use super::{Message, Route};
use crate::backend::{self, inference::LoadedModel};
use crate::error::CmdResult;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub enum InstallEvent {
    Progress(DownloadProgress),
    Done(CmdResult<InstalledInfo>),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Installed(Vec<ModelMetadata>),
    Recommendations(Vec<ModelRecommendation>),
    Install(InstallEvent),
    Loaded {
        open_chat: bool,
        result: CmdResult<LoadedModel>,
    },
}

pub struct InstallJob {
    pub model_id: String,
    pub progress: Option<DownloadProgress>,
}

#[derive(Default)]
pub struct Shared {
    pub installed: Vec<ModelMetadata>,
    pub installed_loaded: bool,

    pub loaded: Vec<ModelInfo>,
    /// The model the chat sends to. Set when a model finishes loading.
    pub active_handle: Option<u64>,

    pub recommendations: Option<Vec<ModelRecommendation>>,
    pub recommendations_loading: bool,

    pub install: Option<InstallJob>,
    pub install_error: Option<String>,

    /// The model being loaded into the engine, if any.
    pub loading_model: Option<String>,
    pub load_error: Option<String>,
}

fn shared(msg: Msg) -> Message {
    Message::Shared(msg)
}

impl Shared {
    /// Everything pages show on first paint.
    pub fn bootstrap(&mut self, state: &Arc<AppState>) -> Task<Message> {
        self.refresh_loaded(state);
        Task::batch([
            self.refresh_installed(state),
            self.refresh_recommendations(state),
        ])
    }

    pub fn refresh_installed(&self, state: &Arc<AppState>) -> Task<Message> {
        let s = Arc::clone(state);
        Task::perform(
            async move { backend::registry::list_installed(&s).await },
            |m| shared(Msg::Installed(m)),
        )
    }

    pub fn refresh_loaded(&mut self, state: &AppState) {
        self.loaded = backend::inference::list_loaded_models(state);
        let still_loaded = self
            .active_handle
            .is_some_and(|h| self.loaded.iter().any(|m| m.handle_id == h));
        if !still_loaded {
            self.active_handle = self.loaded.first().map(|m| m.handle_id);
        }
    }

    pub fn refresh_recommendations(&mut self, state: &Arc<AppState>) -> Task<Message> {
        self.recommendations_loading = true;
        let s = Arc::clone(state);
        Task::perform(
            async move { backend::profile::get_recommendations(&s).await },
            |r| shared(Msg::Recommendations(r)),
        )
    }

    /// Start a download. Only one runs at a time, as in the web app.
    pub fn install(
        &mut self,
        state: &Arc<AppState>,
        model_id: &str,
        quant: Option<String>,
    ) -> Task<Message> {
        if self.install.is_some() {
            return Task::none();
        }
        self.install_error = None;
        self.install = Some(InstallJob {
            model_id: model_id.to_string(),
            progress: None,
        });

        let s = Arc::clone(state);
        let id = model_id.to_string();
        let events = iced::stream::channel(16, async move |mut tx| {
            // Progress arrives on the download's thread through a plain
            // callback; a full channel just drops an update, which is fine
            // for a progress bar.
            let progress_tx = Mutex::new(tx.clone());
            let on_progress = move |p: DownloadProgress| {
                if let Ok(mut t) = progress_tx.lock() {
                    let _ = t.try_send(InstallEvent::Progress(p));
                }
            };
            let result =
                backend::registry::install_model(&s, &id, quant.as_deref(), on_progress).await;
            let _ = tx.send(InstallEvent::Done(result)).await;
        });
        Task::run(events, |e| shared(Msg::Install(e)))
    }

    /// Load an installed model into the engine. With `open_chat`, go to the
    /// chat once it is ready.
    pub fn load_model(
        &mut self,
        state: &Arc<AppState>,
        model_id: &str,
        open_chat: bool,
    ) -> Task<Message> {
        if self.loading_model.is_some() {
            return Task::none();
        }
        self.load_error = None;
        self.loading_model = Some(model_id.to_string());
        let s = Arc::clone(state);
        let id = model_id.to_string();
        Task::perform(
            async move { backend::inference::load_model(&s, &id, None, None).await },
            move |result| shared(Msg::Loaded { open_chat, result }),
        )
    }

    pub fn update(
        &mut self,
        msg: Msg,
        state: &Arc<AppState>,
        toasts: &mut Toasts,
        nav: &mut Option<Route>,
    ) -> Task<Message> {
        match msg {
            Msg::Installed(models) => {
                self.installed = models;
                self.installed_loaded = true;
            }
            Msg::Recommendations(recs) => {
                self.recommendations = Some(recs);
                self.recommendations_loading = false;
            }
            Msg::Install(InstallEvent::Progress(p)) => {
                if let Some(job) = &mut self.install {
                    job.progress = Some(p);
                }
            }
            Msg::Install(InstallEvent::Done(result)) => {
                let model_id = self.install.take().map(|j| j.model_id).unwrap_or_default();
                match result {
                    Ok(_) => {
                        toasts.success(format!("Installed {model_id}."));
                        return self.refresh_installed(state);
                    }
                    Err(e) => {
                        toasts.error(format!("Install failed: {e}"));
                        self.install_error = Some(e);
                    }
                }
            }
            Msg::Loaded { open_chat, result } => {
                self.loading_model = None;
                match result {
                    Ok(loaded) => {
                        self.refresh_loaded(state);
                        self.active_handle = Some(loaded.handle_id);
                        if open_chat {
                            *nav = Some(Route::Chat);
                        }
                    }
                    Err(e) => {
                        toasts.error(format!("Could not load the model: {e}"));
                        self.load_error = Some(e);
                    }
                }
            }
        }
        Task::none()
    }
}
