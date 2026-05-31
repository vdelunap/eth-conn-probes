use crate::model;

pub struct BeaconHttpsProbe {
    pub url: String,
}

#[async_trait::async_trait]
impl super::ProbeFn for BeaconHttpsProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return model::AttemptResult {
                    ok: false,
                    rtt_ms: None,
                    error: Some(format!("client_build: {e}")),
                    meta: serde_json::json!({ "category": "internal" }),
                }
            }
        };

        // Cheapest read-only call in the Beacon API, and public nodes don't gate it.
        let endpoint = format!("{}/eth/v1/node/version", self.url.trim_end_matches('/'));
        let resp = client.get(&endpoint).send().await;

        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&text);

                let (ok, error, category) = match status {
                    401 | 403 => (
                        false,
                        Some(format!("auth required (HTTP {status})")),
                        "auth_required",
                    ),
                    429 => (
                        false,
                        Some("rate limited (HTTP 429)".to_string()),
                        "rate_limited",
                    ),
                    // Beacon API success looks like {"data": {"version": "..."}}
                    200 => match parsed.as_ref().ok() {
                        Some(v) if v.get("data").and_then(|d| d.get("version")).is_some() => {
                            (true, None, "ok")
                        }
                        _ => (
                            false,
                            Some("unexpected response (no data.version field)".to_string()),
                            "api_error",
                        ),
                    },
                    _ => (false, Some(format!("HTTP {status}")), "http_error"),
                };

                model::AttemptResult {
                    ok,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error,
                    meta: serde_json::json!({
                        "status": status,
                        "category": category,
                        "response": parsed.unwrap_or(serde_json::json!({ "raw": text }))
                    }),
                }
            }
            Err(e) => {
                let msg = e.to_string();
                let (category, error) = if msg.contains("timed out") || msg.contains("timeout") {
                    ("timeout", "connection timed out".to_string())
                } else if msg.contains("refused") {
                    ("network", "connection refused".to_string())
                } else if msg.contains("dns") || msg.contains("resolve") || msg.contains("lookup") {
                    ("dns_error", "DNS resolution failed".to_string())
                } else {
                    ("network", format!("network error: {e}"))
                };
                model::AttemptResult {
                    ok: false,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: Some(error),
                    meta: serde_json::json!({ "category": category }),
                }
            }
        }
    }
}
