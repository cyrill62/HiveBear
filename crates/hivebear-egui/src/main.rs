#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backend;
mod error;
mod state;
mod tasks;
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

    // One multi-threaded runtime for every backend call. The UI thread never
    // blocks on it: work is spawned and polled once per frame (see `tasks`).
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("hivebear-rt")
        .build()
    {
        Ok(rt) => rt,
        Err(e) => fail_startup(&format!("Could not start the async runtime.\n\n{e}")),
    };

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("HiveBear")
        .with_app_id("com.hivebear.app")
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([900.0, 600.0]);
    match eframe::icon_data::from_png_bytes(WINDOW_ICON) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => tracing::warn!("Could not decode the window icon: {e}"),
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let handle = runtime.handle().clone();
    let result = eframe::run_native(
        "HiveBear",
        options,
        Box::new(move |cc| Ok(Box::new(ui::HiveBearApp::new(cc, handle)))),
    );

    if let Err(e) = result {
        error!("HiveBear exited with an error: {e}");
        eprintln!("HiveBear exited with an error: {e}");
        write_startup_failure(&e.to_string());
        std::process::exit(1);
    }

    // Give in-flight work (a mesh deregistration, a usage event) a moment to
    // finish instead of cutting it off mid-request.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}

/// Log, record and report a fatal startup error, then exit.
fn fail_startup(message: &str) -> ! {
    error!("HiveBear could not start: {message}");
    eprintln!("HiveBear could not start:\n{message}");
    write_startup_failure(message);
    std::process::exit(1);
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
