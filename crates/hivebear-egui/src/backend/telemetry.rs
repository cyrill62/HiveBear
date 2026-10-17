use hivebear_core::Config;

/// Record that the first-run telemetry notice has been shown.
///
/// Opt-out reporting is only defensible if people are actually told, and this
/// is a GUI app with no console — so the notice lives in the welcome screen
/// and this is how it says it has done its job.
pub fn acknowledge_notice() -> Result<(), String> {
    let mut config = Config::load();
    config.telemetry.notice_shown = true;
    config.save().map_err(|e| e.to_string())
}

/// Send `first_launch`. Called when the welcome screen closes rather than at
/// startup, so the "anonymous usage counts" switch on that screen is honoured
/// before anything is sent.
pub async fn record_first_launch() {
    hivebear_core::usage::send_usage_event_once(hivebear_core::usage::EVENT_FIRST_LAUNCH).await;
}
