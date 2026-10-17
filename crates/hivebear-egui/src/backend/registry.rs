use crate::error::{CmdResult, CommandError};
use crate::state::AppState;
use hivebear_registry::download::DownloadProgress;
use hivebear_registry::{metadata::SearchResult, InstalledInfo, ModelMetadata, StorageReport};

pub async fn search_models(state: &AppState, query: &str) -> CmdResult<Vec<SearchResult>> {
    state
        .registry
        .search(query, 20, Some(&state.profile))
        .await
        .map_err(|e| CommandError::from(e).into())
}

pub async fn install_model(
    state: &AppState,
    model_id: &str,
    quant: Option<&str>,
    on_progress: impl Fn(DownloadProgress) + Send + Sync,
) -> CmdResult<InstalledInfo> {
    let info = state
        .registry
        .install(model_id, quant, None, Some(&on_progress))
        .await
        .map_err(CommandError::from)?;
    tokio::spawn(hivebear_core::usage::send_usage_event(
        hivebear_core::usage::EVENT_MODEL_INSTALLED,
    ));
    Ok(info)
}

pub async fn list_installed(state: &AppState) -> Vec<ModelMetadata> {
    state.registry.list_installed().await
}

pub async fn remove_model(state: &AppState, model_id: &str) -> CmdResult<u64> {
    state
        .registry
        .remove(model_id)
        .await
        .map_err(|e| CommandError::from(e).into())
}

pub async fn get_storage_report(state: &AppState) -> CmdResult<StorageReport> {
    state
        .registry
        .storage_manager()
        .await
        .report()
        .await
        .map_err(|e| e.to_string())
}
