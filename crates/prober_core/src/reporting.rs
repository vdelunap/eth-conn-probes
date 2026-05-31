use crate::model::Report;

pub async fn send_report(report: &Report, url: &str, timeout_ms: u64) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()?;

    let resp = client.post(url).json(report).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("report failed: status={status} body={body}");
    }
    Ok(())
}

pub async fn fetch_geo_reports(
    report_url: &str,
    kinds: &[String],
    timeout_ms: u64,
) -> anyhow::Result<serde_json::Value> {
    let base = report_url.trim_end_matches("/report");
    let url = if kinds.is_empty() {
        format!("{base}/api/geo-reports")
    } else {
        format!("{base}/api/geo-reports?kinds={}", kinds.join(","))
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()?;

    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("geo-reports failed: HTTP {}", resp.status());
    }
    Ok(resp.json().await?)
}
