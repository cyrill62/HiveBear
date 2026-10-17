use crate::state::AppState;
use hivebear_core::types::CommunityBenchmarkSummary;
use hivebear_core::{HardwareFingerprint, ModelRecommendation};

/// Model recommendations for this machine, blended with community
/// benchmark data when the user shares benchmarks.
pub async fn get_recommendations(state: &AppState) -> Vec<ModelRecommendation> {
    let config = state.config();

    if !config.share_benchmarks {
        return hivebear_core::recommender::recommend(&state.profile, &config);
    }

    let fp = HardwareFingerprint::from_profile(&state.profile);
    let url = format!(
        "{}/benchmarks?gpu_class={}&ram_gb_bucket={}&platform_arch={}",
        config.mesh.coordination_server, fp.gpu_class, fp.ram_gb_bucket, fp.platform_arch
    );

    #[derive(serde::Deserialize)]
    struct QueryResponse {
        results: Vec<CommunityBenchmarkSummary>,
    }

    let community_data = match state.http_client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => resp
            .json::<QueryResponse>()
            .await
            .map(|d| d.results)
            .unwrap_or_default(),
        _ => vec![],
    };

    hivebear_core::recommender::recommend_with_community(&state.profile, &config, &community_data)
}
