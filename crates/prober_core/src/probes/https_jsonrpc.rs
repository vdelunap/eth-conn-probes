use crate::model;

pub struct HttpsJsonRpcProbe {
    pub url: String,
    pub method: String,
}

#[async_trait::async_trait]
impl super::ProbeFn for HttpsJsonRpcProbe {
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

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": self.method,
            "params": []
        });

        let resp = client.post(&self.url).json(&payload).send().await;

        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r.text().await.unwrap_or_default();
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&text);

                // Determine outcome based on HTTP status and JSON-RPC response body.
                let (ok, error, category) = match status {
                    // Authentication/authorization failure.
                    401 | 403 => (
                        false,
                        Some(format!("auth required (HTTP {status})")),
                        "auth_required",
                    ),
                    // Rate limiting — free-tier providers (Ankr, LlamaRPC) sometimes do this.
                    429 => (
                        false,
                        Some("rate limited (HTTP 429)".to_string()),
                        "rate_limited",
                    ),
                    // 200 OK — check the JSON-RPC response body.
                    200 => match parsed.as_ref().ok() {
                        // Valid JSON-RPC success response.
                        Some(v) if v.get("result").is_some() => (true, None, "ok"),
                        // Valid JSON-RPC error response (provider accepted the request but
                        // returned an application-level error, e.g. method not allowed).
                        Some(v) if v.get("error").is_some() => {
                            let err_obj = &v["error"];
                            let msg = err_obj["message"]
                                .as_str()
                                .unwrap_or("unknown error");
                            let code = err_obj["code"].as_i64().unwrap_or(0);
                            (
                                false,
                                Some(format!("RPC error {code}: {msg}")),
                                "rpc_error",
                            )
                        }
                        // Response is 200 but the body is not valid JSON or missing fields.
                        _ => (
                            false,
                            Some("unexpected response (no result or error field)".to_string()),
                            "api_error",
                        ),
                    },
                    // Any other HTTP error (5xx server errors, etc.).
                    _ => (
                        false,
                        Some(format!("HTTP {status}")),
                        "http_error",
                    ),
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
            // Network-level failure: the request never reached the server.
            Err(e) => {
                let msg = e.to_string();
                // Classify common connection errors for better reporting.
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
