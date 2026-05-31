mod commands;
mod queue;

/// Mobile entry point. Mirrors what main.rs does on desktop.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::run_probes,
            commands::flush_queued_reports,
            commands::get_geo_reports
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
