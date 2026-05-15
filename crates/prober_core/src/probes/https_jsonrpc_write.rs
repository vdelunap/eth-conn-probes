use crate::model;

pub struct HttpsJsonRpcWriteProbe {
    pub url: String,
}

#[async_trait::async_trait]
impl super::ProbeFn for HttpsJsonRpcWriteProbe {
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
                    meta: serde_json::json!({"category": "internal"}),
                }
            }
        };

        // Send eth_sendRawTransaction with an intentionally invalid payload ("0x").
        // A functioning provider returns a JSON-RPC error (e.g. -32000 "invalid transaction").
        // A write-censored endpoint returns a network error, 403, or no response at all.
        // We treat *any* valid JSON-RPC response (including errors) as success: it proves
        // the write endpoint is accessible and processing requests.
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_sendRawTransaction",
            "params": ["0x"]
        });

        let resp = client.post(&self.url).json(&payload).send().await;

        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&text);

                // Check for a JSON-RPC body first, regardless of HTTP status.
                // Some providers (e.g. Flashbots) return HTTP 400 with a JSON error body
                // instead of HTTP 200 + JSON-RPC error. Both prove the write endpoint is
                // alive and processing requests — only network-level failures are censorship.
                let (ok, error, category) =
                    if let Some(v) = parsed.as_ref().ok() {
                        if v.get("error").is_some() || v.get("result").is_some() {
                            (true, None, "ok")
                        } else {
                            (false, Some(format!("unexpected body (HTTP {status})")), "api_error")
                        }
                    } else {
                        match status {
                            401 | 403 => (
                                false,
                                Some(format!("auth required (HTTP {status})")),
                                "auth_required",
                            ),
                            429 => (false, Some("rate limited (HTTP 429)".into()), "rate_limited"),
                            _ => (false, Some(format!("HTTP {status}")), "http_error"),
                        }
                    };

                model::AttemptResult {
                    ok,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error,
                    meta: serde_json::json!({
                        "status":   status,
                        "category": category,
                        "response": parsed.unwrap_or(serde_json::json!({"raw": text}))
                    }),
                }
            }
            Err(e) => {
                let msg = e.to_string();
                let (category, error) = if msg.contains("timed out") || msg.contains("timeout") {
                    ("timeout", "connection timed out".into())
                } else if msg.contains("refused") {
                    ("network", "connection refused".into())
                } else if msg.contains("dns") || msg.contains("resolve") {
                    ("dns_error", "DNS resolution failed".into())
                } else {
                    ("network", format!("network error: {e}"))
                };
                model::AttemptResult {
                    ok: false,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: Some(error),
                    meta: serde_json::json!({"category": category}),
                }
            }
        }
    }
}
