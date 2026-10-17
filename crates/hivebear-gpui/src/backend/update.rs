//! Checking for a newer release.
//!
//! Reads the same `latest.json` manifest the Tauri updater publishes. The
//! egui build does not replace itself in place: the packaged formats differ
//! per platform (AppImage, MSI, DMG, Homebrew, Scoop, AUR), and a
//! self-replacing binary would fight every one of those package managers. So
//! this only tells the user a release exists and where to get it.

use serde::Deserialize;

const MANIFEST_URL: &str =
    "https://github.com/BeckhamLabsLLC/HiveBear/releases/latest/download/latest.json";
pub const RELEASES_URL: &str = "https://github.com/BeckhamLabsLLC/HiveBear/releases/latest";

#[derive(Deserialize)]
struct Manifest {
    version: String,
}

/// The newer version, if one has been published.
pub async fn check(client: &reqwest::Client) -> Result<Option<String>, String> {
    let manifest: Manifest = client
        .get(MANIFEST_URL)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Update check failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Update manifest unreadable: {e}"))?;

    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| format!("Bad build version: {e}"))?;
    let latest = semver::Version::parse(manifest.version.trim_start_matches('v'))
        .map_err(|e| format!("Bad manifest version '{}': {e}", manifest.version))?;

    Ok((latest > current).then(|| latest.to_string()))
}
