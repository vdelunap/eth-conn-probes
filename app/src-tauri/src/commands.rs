use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SendOutcome {
    Sent,
    Queued { reason: String },
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub report: prober_core::model::Report,
    pub send: SendOutcome,
}

#[tauri::command]
pub async fn run_plan_toml(
    app: tauri::AppHandle,
    config_toml: String,
    no_send: bool,
) -> Result<String, String> {
    let cfg = prober_core::config::Config::from_toml_str(&config_toml).map_err(|e| e.to_string())?;
    let report = prober_core::run_plan(cfg.clone()).await.map_err(|e| e.to_string())?;

    let send = if !cfg.reporting.enabled || no_send {
        SendOutcome::Sent
    } else {
        match prober_core::reporting::send_report(&report, &cfg.reporting.report_url, cfg.reporting.timeout_ms).await {
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
    };

    let payload = RunResponse { report, send };
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn flush_queued_reports(app: tauri::AppHandle) -> Result<String, String> {
    let flushed = crate::queue::flush_queue(&app).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "flushed": flushed }).to_string())
}
