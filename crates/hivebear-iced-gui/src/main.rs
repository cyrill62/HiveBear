#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod error;
mod state;
mod telemetry;
mod ui;
mod validation;

use tracing::error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

const WINDOW_ICON: &[u8] = include_bytes!("../assets/icon.png");

fn main() {
    // Before anything that can fail. Config is read directly rather than through
    // AppState because AppState::init_with_paths is itself one of the things that fails.
    let _sentry = telemetry::init(&mut hivebear_core::Config::load());

    // Composed as a registry so the Sentry layer sits beside the formatter. The
    // release build has no console on Windows, so for real users the formatter
    // goes nowhere and Sentry is what actually sees these.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with(tracing_subscriber::fmt::layer())
        .with(sentry::integrations::tracing::layer())
        .init();

    let icon = match iced::window::icon::from_file_data(WINDOW_ICON, None) {
        Ok(icon) => Some(icon),
        Err(e) => {
            tracing::warn!("Could not decode the window icon: {e}");
            None
        }
    };

    let window = iced::window::Settings {
        size: iced::Size::new(1200.0, 800.0),
        min_size: Some(iced::Size::new(900.0, 600.0)),
        icon,
        // Closing first deregisters from the mesh; the app exits itself after.
        exit_on_close_request: false,
        #[cfg(target_os = "linux")]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: "com.hivebear.app".into(),
            ..Default::default()
        },
        ..Default::default()
    };

    // iced owns the Tokio runtime (the `tokio` feature); every backend call
    // runs on it as a `Task`.
    let result = iced::application(ui::App::boot, ui::App::update, ui::App::view)
        .title("HiveBear")
        .theme(|_: &ui::App| ui::theme::app_theme())
        .subscription(ui::App::subscription)
        .font(ui::icon::FONT_BYTES)
        .window(window)
        .run();

    if let Err(e) = result {
        error!("HiveBear exited with an error: {e}");
        eprintln!("HiveBear exited with an error: {e}");
        write_startup_failure(&e.to_string());
        std::process::exit(1);
    }
}

/// Record a startup failure where a user can actually be pointed at it.
/// The data directory may itself be the thing that is broken, so fall back to
/// the temp directory.
///
/// Also reports it. A local log file only helps a user who knows to look for it
/// and knows how to send it to us; for a startup crash — where there is no
/// window and nothing on screen — that is close to nobody.
pub fn write_startup_failure(message: &str) {
    telemetry::capture_fatal(message);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!(
        "HiveBear {} startup failure\n\n{message}\n",
        env!("CARGO_PKG_VERSION")
    );

    let candidates = [
        hivebear_core::AppPaths::new()
            .data_dir
            .join("startup-error.log"),
        std::env::temp_dir().join(format!("hivebear-startup-error-{stamp}.log")),
    ];
    for path in candidates {
        if std::fs::write(&path, &body).is_ok() {
            eprintln!("Details written to {}", path.display());
            return;
        }
    }
}
