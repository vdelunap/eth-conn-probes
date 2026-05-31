use serde::Serialize;
use tauri::Manager;

// --- Response types ---

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

fn load_or_create_client_id(app: &tauri::AppHandle) -> String {
    let path = match app
        .path()
        .resolve("client_id.txt", tauri::path::BaseDirectory::AppData)
    {
        Ok(p) => p,
        Err(_) => return uuid::Uuid::new_v4().to_string(),
    };

    if let Ok(id) = std::fs::read_to_string(&path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &id);
    id
}

// --- Commands ---

#[tauri::command]
pub async fn run_probes(
    app: tauri::AppHandle,
    no_send: bool,
    network_label: Option<String>,
) -> Result<String, String> {
    let mut cfg = prober_core::config::default_config();
    cfg.client.client_id = load_or_create_client_id(&app);
    cfg.client.network_label = network_label.filter(|s| !s.trim().is_empty());
    if no_send {
        cfg.reporting.enabled = false;
    }

    let report = prober_core::run_plan(cfg.clone())
        .await
        .map_err(|e| e.to_string())?;

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

#[tauri::command]
pub async fn flush_queued_reports(app: tauri::AppHandle) -> Result<String, String> {
    let flushed = crate::queue::flush_queue(&app)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "flushed": flushed }).to_string())
}

#[tauri::command]
pub async fn get_geo_reports(kinds: Vec<String>) -> Result<String, String> {
    let cfg = prober_core::config::default_config();
    let data = prober_core::reporting::fetch_geo_reports(
        &cfg.reporting.report_url,
        &kinds,
        cfg.reporting.timeout_ms,
    )
    .await
    .map_err(|e| e.to_string())?;
    serde_json::to_string(&data).map_err(|e| e.to_string())
}
