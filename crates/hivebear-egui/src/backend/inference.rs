use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::CmdResult;
use crate::state::AppState;
use hivebear_inference::types::{ChatMessage, ModelInfo};
use hivebear_inference::ModelHandle;

#[derive(Clone)]
pub struct LoadedModel {
    pub handle_id: u64,
}

pub async fn load_model(
    state: &AppState,
    model_id: &str,
    context_length: Option<u32>,
    gpu_layers: Option<u32>,
) -> CmdResult<LoadedModel> {
    let mut load_config = hivebear_inference::LoadConfig::default();
    if let Some(ctx) = context_length {
        load_config.context_length = ctx;
    }
    if let Some(layers) = gpu_layers {
        load_config.offload.gpu_layers = Some(layers);
        load_config.offload.auto = false;
    }

    let resolved = state
        .registry
        .resolve(model_id)
        .await
        .map_err(|e| format!("Failed to resolve model '{model_id}': {e}"))?;

    let handle = state
        .orchestrator
        .load(&resolved, &load_config)
        .await
        .map_err(|e| String::from(crate::error::CommandError::from(e)))?;

    Ok(LoadedModel {
        handle_id: handle.id,
    })
}

/// Sampling options the chat panel exposes.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatParams {
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
}

impl Default for ChatParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            max_tokens: 2048,
        }
    }
}

/// How a generation ended.
pub enum StreamEnd {
    /// The model finished, or `cancel` was set; carries the text produced.
    Done(String),
    /// Generation failed part-way; carries the error.
    Failed(String),
}

/// Stream a reply, handing each token to `on_token` as it arrives.
///
/// Setting `cancel` stops generation at the next token — the stream is
/// dropped, which stops the engine, rather than the old behaviour of the UI
/// ignoring tokens while the model kept running.
pub async fn stream_chat(
    state: &AppState,
    handle_id: u64,
    messages: Vec<ChatMessage>,
    params: ChatParams,
    cancel: &AtomicBool,
    mut on_token: impl FnMut(&str),
) -> CmdResult<StreamEnd> {
    use futures::StreamExt;

    let handle = handle_for(state, handle_id)?;
    let model_name = handle.model_path.display().to_string();

    let mut request = hivebear_inference::GenerateRequest {
        messages,
        model_name: Some(model_name),
        ..Default::default()
    };
    request.sampling.temperature = params.temperature;
    request.sampling.top_p = params.top_p;
    request.max_tokens = params.max_tokens;

    let mut stream = state
        .orchestrator
        .stream(&handle, &request)
        .map_err(|e| String::from(crate::error::CommandError::from(e)))?;

    let mut full_text = String::new();
    while let Some(result) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match result {
            Ok(token) => {
                full_text.push_str(&token.text);
                on_token(&token.text);
            }
            Err(e) => return Ok(StreamEnd::Failed(e.to_string())),
        }
    }

    if !full_text.is_empty() {
        // Once per install, and a no-op if usage counts are turned off.
        tokio::spawn(hivebear_core::usage::send_usage_event_once(
            hivebear_core::usage::EVENT_FIRST_INFERENCE,
        ));
    }

    Ok(StreamEnd::Done(full_text))
}

pub fn list_loaded_models(state: &AppState) -> Vec<ModelInfo> {
    state.orchestrator.loaded_models()
}

fn handle_for(state: &AppState, handle_id: u64) -> CmdResult<ModelHandle> {
    let models = state.orchestrator.loaded_models();
    let info = models
        .iter()
        .find(|m| m.handle_id == handle_id)
        .ok_or_else(|| String::from("Model not loaded"))?;
    Ok(ModelHandle {
        id: handle_id,
        model_path: info.model_path.clone().into(),
        engine: info.engine,
    })
}
