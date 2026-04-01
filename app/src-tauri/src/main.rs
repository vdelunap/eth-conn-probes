mod commands;
mod queue;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::run_probes,
            commands::flush_queued_reports
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
