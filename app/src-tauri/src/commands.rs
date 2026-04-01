use serde::Serialize;
use tauri::Manager;

// --- Response types ---

/// Whether the report was sent, queued for later, or skipped by the user.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SendOutcome {
    Sent,
    Queued { reason: String },
    Skipped,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub report: prober_core::model::Report,
    pub send: SendOutcome,
}

// --- client_id persistence ---

/// Returns the persistent client ID for this device.
///
/// On first call a random UUID is generated and written to
/// `<AppData>/client_id.txt`. On subsequent calls the stored value is returned.
/// This lets us correlate reports from the same device across runs without
/// collecting any personally identifiable information.
fn load_or_create_client_id(app: &tauri::AppHandle) -> String {
    let path = match app
        .path()
        .resolve("client_id.txt", tauri::path::BaseDirectory::AppData)
    {
        Ok(p) => p,
        Err(_) => return uuid::Uuid::new_v4().to_string(),
    };

    // Try to read an existing ID from disk.
    if let Ok(id) = std::fs::read_to_string(&path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    // First launch: generate, persist, and return a fresh ID.
    let id = uuid::Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &id);
    id
}

// --- Commands ---

/// Runs all probes using the hardcoded default configuration.
///
/// `no_send` — if true, the report is kept local (user opted out).
#[tauri::command]
pub async fn run_probes(app: tauri::AppHandle, no_send: bool) -> Result<String, String> {
    // Load the hardcoded default config and inject the persistent client ID.
    let mut cfg = prober_core::config::default_config();
    cfg.client.client_id = load_or_create_client_id(&app);
    if no_send {
        cfg.reporting.enabled = false;
    }

    // Run all probes concurrently.
    let report = prober_core::run_plan(cfg.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Try to send the report; if it fails, queue it locally for a later retry.
    let send = if cfg.reporting.enabled {
        match prober_core::reporting::send_report(
            &report,
            &cfg.reporting.report_url,
            cfg.reporting.timeout_ms,
        )
        .await
        {
            Ok(_) => SendOutcome::Sent,
            Err(e) => {
                let reason = e.to_string();
                if let Err(qe) = crate::queue::append_queued_report(&app, &report).await {
                    SendOutcome::Queued {
                        reason: format!("{reason}; queue_failed={qe}"),
                    }
                } else {
                    SendOutcome::Queued { reason }
                }
            }
        }
    } else {
        SendOutcome::Skipped
    };

    let payload = RunResponse { report, send };
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

/// Attempts to re-send all locally queued reports.
#[tauri::command]
pub async fn flush_queued_reports(app: tauri::AppHandle) -> Result<String, String> {
    let flushed = crate::queue::flush_queue(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "flushed": flushed }).to_string())
}
