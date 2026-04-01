mod commands;
mod queue;

/// Mobile entry point — mirrors what main.rs does on desktop.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::run_probes,
            commands::flush_queued_reports
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
