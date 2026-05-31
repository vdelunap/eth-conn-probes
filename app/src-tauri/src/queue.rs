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
        // TODO: actually re-POST these. Queue entries would need to carry the
        // report_url they were meant for, since a Report doesn't embed it.
        let _report: prober_core::model::Report = serde_json::from_str(line)?;
        flushed += 1;
    }

    std::fs::write(&path, "")?;
    Ok(flushed)
}
