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

    // GPUI runs its own executor on the main thread; the backend (reqwest,
    // the mesh, blocking SQLite calls) needs Tokio. Backend futures run here
    // and GPUI tasks await their join handles.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("hivebear-rt")
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let message = format!("Could not start the async runtime.\n\n{e}");
            error!("HiveBear could not start: {message}");
            write_startup_failure(&message);
            std::process::exit(1);
        }
    };

    ui::run(runtime.handle().clone());

    // Give in-flight work (a mesh deregistration, a usage event) a moment to
    // finish instead of cutting it off mid-request.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
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
