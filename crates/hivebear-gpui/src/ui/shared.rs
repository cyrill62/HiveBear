//! State more than one page reads: what is installed, what is loaded, the
//! running download. It lives once, above the pages, so a download keeps its
//! progress while you look elsewhere.

use std::sync::{Arc, Mutex};

use gpui_kit::*;
use hivebear_core::ModelRecommendation;
use hivebear_inference::types::ModelInfo;
use hivebear_registry::download::DownloadProgress;
use hivebear_registry::ModelMetadata;

use super::{call, cmd, rt, toast_err, toast_ok, Navigate, Route};
use crate::backend;
use crate::state::AppState;

pub struct InstallJob {
    pub model_id: String,
    pub progress: Option<DownloadProgress>,
}

pub struct Shared {
    pub state: Arc<AppState>,

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

    _install_task: Option<Task<()>>,
}

impl EventEmitter<Navigate> for Shared {}

impl Shared {
    pub fn new(state: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            state,
            installed: vec![],
            installed_loaded: false,
            loaded: vec![],
            active_handle: None,
            recommendations: None,
            recommendations_loading: false,
            install: None,
            install_error: None,
            loading_model: None,
            load_error: None,
            _install_task: None,
        };
        this.refresh_loaded(cx);
        this.refresh_installed(cx);
        this.refresh_recommendations(cx);
        this
    }

    pub fn refresh_installed(&mut self, cx: &mut Context<Self>) {
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let Some(models) = call(
                rt,
                async move { backend::registry::list_installed(&s).await },
            )
            .await
            else {
                return;
            };
            this.update(cx, |this, cx| {
                this.installed = models;
                this.installed_loaded = true;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn refresh_loaded(&mut self, cx: &mut Context<Self>) {
        self.loaded = backend::inference::list_loaded_models(&self.state);
        let still_loaded = self
            .active_handle
            .is_some_and(|h| self.loaded.iter().any(|m| m.handle_id == h));
        if !still_loaded {
            self.active_handle = self.loaded.first().map(|m| m.handle_id);
        }
        cx.notify();
    }

    pub fn refresh_recommendations(&mut self, cx: &mut Context<Self>) {
        self.recommendations_loading = true;
        let s = Arc::clone(&self.state);
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let recs = call(rt, async move {
                backend::profile::get_recommendations(&s).await
            })
            .await;
            this.update(cx, |this, cx| {
                this.recommendations = Some(recs.unwrap_or_default());
                this.recommendations_loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Start a download. Only one runs at a time, as in the web app.
    pub fn install(&mut self, model_id: &str, quant: Option<String>, cx: &mut Context<Self>) {
        if self.install.is_some() {
            return;
        }
        self.install_error = None;
        self.install = Some(InstallJob {
            model_id: model_id.to_string(),
            progress: None,
        });
        cx.notify();

        // Progress arrives on the download's thread through a plain callback;
        // the GPUI task polls the latest value rather than every update.
        let latest: Arc<Mutex<Option<DownloadProgress>>> = Arc::default();
        let sink = Arc::clone(&latest);
        let s = Arc::clone(&self.state);
        let id = model_id.to_string();
        let rt = rt(cx);
        let download = rt.spawn(async move {
            let on_progress = move |p: DownloadProgress| {
                *sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
            };
            backend::registry::install_model(&s, &id, quant.as_deref(), on_progress).await
        });

        self._install_task = Some(cx.spawn(async move |this, cx| {
            let mut download = download;
            let result = loop {
                let tick = cx
                    .background_executor()
                    .timer(std::time::Duration::from_millis(250));
                futures::pin_mut!(tick);
                match futures::future::select(&mut download, tick).await {
                    futures::future::Either::Left((result, _)) => {
                        break result.unwrap_or_else(|e| {
                            Err(format!("The download stopped unexpectedly: {e}"))
                        });
                    }
                    futures::future::Either::Right(_) => {
                        let progress = latest.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        let alive = this
                            .update(cx, |this, cx| {
                                if let Some(job) = &mut this.install {
                                    job.progress = progress;
                                }
                                cx.notify();
                            })
                            .is_ok();
                        if !alive {
                            return;
                        }
                    }
                }
            };
            this.update(cx, |this, cx| {
                let model_id = this.install.take().map(|j| j.model_id).unwrap_or_default();
                match result {
                    Ok(_) => {
                        toast_ok(cx, format!("Installed {model_id}."));
                        this.refresh_installed(cx);
                    }
                    Err(e) => {
                        toast_err(cx, format!("Install failed: {e}"));
                        this.install_error = Some(e);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Load an installed model into the engine. With `open_chat`, go to the
    /// chat once it is ready.
    pub fn load_model(&mut self, model_id: &str, open_chat: bool, cx: &mut Context<Self>) {
        if self.loading_model.is_some() {
            return;
        }
        self.load_error = None;
        self.loading_model = Some(model_id.to_string());
        cx.notify();
        let s = Arc::clone(&self.state);
        let id = model_id.to_string();
        let rt = rt(cx);
        cx.spawn(async move |this, cx| {
            let result = cmd(rt, async move {
                backend::inference::load_model(&s, &id, None, None).await
            })
            .await;
            this.update(cx, |this, cx| {
                this.loading_model = None;
                match result {
                    Ok(loaded) => {
                        this.refresh_loaded(cx);
                        this.active_handle = Some(loaded.handle_id);
                        if open_chat {
                            cx.emit(Navigate(Route::Chat));
                        }
                    }
                    Err(e) => {
                        toast_err(cx, format!("Could not load the model: {e}"));
                        this.load_error = Some(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
