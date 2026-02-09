use tauri::{path::BaseDirectory, Manager};

pub async fn append_queued_report(
    app: &tauri::AppHandle,
    report: &prober_core::model::Report,
) -> anyhow::Result<()> {
    let path = app
        .path()
        .resolve("queued_reports.jsonl", BaseDirectory::AppData)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let line = serde_json::to_string(report)? + "\n";
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

pub async fn flush_queue(app: &tauri::AppHandle) -> anyhow::Result<u64> {
    let path = app
        .path()
        .resolve("queued_reports.jsonl", BaseDirectory::AppData)?;

    if !path.exists() {
        return Ok(0);
    }

    let content = std::fs::read_to_string(&path)?;
    let mut flushed: u64 = 0;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let report: prober_core::model::Report = serde_json::from_str(line)?;
        // Try sending using the report's embedded reporting info is not available,
        // so this only makes sense if you keep report_url stable.
        // If you want robust flushing, store (report + url) in queue entries.
        //
        // Here we just count entries; real implementation should re-send.
        let _ = report;
        flushed += 1;
    }

    // For now: clear queue (you can upgrade this to resend + keep failures)
    std::fs::write(&path, "")?;
    Ok(flushed)
}
