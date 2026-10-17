//! State more than one page reads: what is installed, what is loaded, the
//! running download. In the web app each page re-fetched these on mount and
//! lost a download's progress when you navigated away; here they live once,
//! above the pages.

use std::sync::{Arc, Mutex};

use hivebear_core::ModelRecommendation;
use hivebear_inference::types::ModelInfo;
use hivebear_registry::download::DownloadProgress;
use hivebear_registry::{InstalledInfo, ModelMetadata};

use super::toast::Toasts;
use super::Route;
use crate::backend;
use crate::error::CmdResult;
use crate::tasks::{poll_slot, Ctx, Task};

pub struct InstallJob {
    pub model_id: String,
    pub progress: Arc<Mutex<Option<DownloadProgress>>>,
    task: Task<CmdResult<InstalledInfo>>,
}

impl InstallJob {
    pub fn progress(&self) -> Option<DownloadProgress> {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

struct LoadJob {
    model_id: String,
    open_chat: bool,
    task: Task<CmdResult<backend::inference::LoadedModel>>,
}

#[derive(Default)]
pub struct Shared {
    pub installed: Vec<ModelMetadata>,
    pub installed_loaded: bool,
    installed_task: Option<Task<Vec<ModelMetadata>>>,

    pub loaded: Vec<ModelInfo>,
    /// The model the chat sends to. Set when a model finishes loading.
    pub active_handle: Option<u64>,

    pub recommendations: Option<Vec<ModelRecommendation>>,
    recs_task: Option<Task<Vec<ModelRecommendation>>>,

    pub install: Option<InstallJob>,
    pub install_error: Option<String>,

    load: Option<LoadJob>,
    pub load_error: Option<String>,
}

impl Shared {
    /// Everything pages show on first paint.
    pub fn bootstrap(&mut self, ctx: &Ctx) {
        self.refresh_installed(ctx);
        self.refresh_loaded(ctx);
        self.refresh_recommendations(ctx);
    }

    pub fn refresh_installed(&mut self, ctx: &Ctx) {
        self.installed_task =
            Some(ctx.run(|s| async move { backend::registry::list_installed(&s).await }));
    }

    pub fn refresh_loaded(&mut self, ctx: &Ctx) {
        self.loaded = backend::inference::list_loaded_models(&ctx.state);
        let still_loaded = self
            .active_handle
            .is_some_and(|h| self.loaded.iter().any(|m| m.handle_id == h));
        if !still_loaded {
            self.active_handle = self.loaded.first().map(|m| m.handle_id);
        }
    }

    pub fn refresh_recommendations(&mut self, ctx: &Ctx) {
        self.recs_task =
            Some(ctx.run(|s| async move { backend::profile::get_recommendations(&s).await }));
    }

    pub fn recommendations_loading(&self) -> bool {
        self.recs_task.is_some()
    }

    /// Start a download. Only one runs at a time, as in the web app.
    pub fn install(&mut self, ctx: &Ctx, model_id: &str, quant: Option<String>) {
        if self.install.is_some() {
            return;
        }
        self.install_error = None;
        let progress = Arc::new(Mutex::new(None));
        let sink = Arc::clone(&progress);
        let egui = ctx.egui.clone();
        let id = model_id.to_string();
        let task = ctx.run(move |s| async move {
            let on_progress = move |p: DownloadProgress| {
                *sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
                egui.request_repaint();
            };
            backend::registry::install_model(&s, &id, quant.as_deref(), on_progress).await
        });
        self.install = Some(InstallJob {
            model_id: model_id.to_string(),
            progress,
            task,
        });
    }

    pub fn installing(&self) -> Option<&str> {
        self.install.as_ref().map(|j| j.model_id.as_str())
    }

    /// Load an installed model into the engine. With `open_chat`, go to the
    /// chat once it is ready.
    pub fn load_model(&mut self, ctx: &Ctx, model_id: &str, open_chat: bool) {
        if self.load.is_some() {
            return;
        }
        self.load_error = None;
        let id = model_id.to_string();
        self.load = Some(LoadJob {
            model_id: id.clone(),
            open_chat,
            task: ctx.run(move |s| async move {
                backend::inference::load_model(&s, &id, None, None).await
            }),
        });
    }

    pub fn model_loading(&self) -> Option<&str> {
        self.load.as_ref().map(|j| j.model_id.as_str())
    }

    /// Collect finished background work. Called once per frame.
    pub fn poll(&mut self, ctx: &Ctx, toasts: &mut Toasts, nav: &mut Option<Route>) {
        if let Some(models) = poll_slot(&mut self.installed_task) {
            self.installed = models;
            self.installed_loaded = true;
        }
        if let Some(recs) = poll_slot(&mut self.recs_task) {
            self.recommendations = Some(recs);
        }

        if let Some(job) = &mut self.install {
            if let Some(result) = job.task.poll() {
                let model_id = job.model_id.clone();
                self.install = None;
                match result {
                    Ok(Ok(_)) => {
                        toasts.success(format!("Installed {model_id}."));
                        self.refresh_installed(ctx);
                    }
                    Ok(Err(e)) => {
                        toasts.error(format!("Install failed: {e}"));
                        self.install_error = Some(e);
                    }
                    Err(()) => {
                        self.install_error = Some("The download stopped unexpectedly.".into())
                    }
                }
            }
        }

        if let Some(job) = &mut self.load {
            if let Some(result) = job.task.poll() {
                let open_chat = job.open_chat;
                self.load = None;
                match result {
                    Ok(Ok(loaded)) => {
                        self.refresh_loaded(ctx);
                        self.active_handle = Some(loaded.handle_id);
                        if open_chat {
                            *nav = Some(Route::Chat);
                        }
                    }
                    Ok(Err(e)) => {
                        toasts.error(format!("Could not load the model: {e}"));
                        self.load_error = Some(e);
                    }
                    Err(()) => self.load_error = Some("Loading stopped unexpectedly.".into()),
                }
            }
        }
    }
}
