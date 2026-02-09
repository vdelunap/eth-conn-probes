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
