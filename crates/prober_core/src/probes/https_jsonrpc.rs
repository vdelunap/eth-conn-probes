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
                    error: Some(format!("client_build_error: {e}")),
                    meta: serde_json::json!({}),
                };
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
                let ok = status == 200
                    && parsed
                        .as_ref()
                        .ok()
                        .and_then(|v| v.get("result"))
                        .is_some();

                model::AttemptResult {
                    ok,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: if ok {
                        None
                    } else {
                        Some(format!("bad_jsonrpc: status={status}"))
                    },
                    meta: serde_json::json!({
                        "status": status,
                        "response": parsed.unwrap_or(serde_json::json!({ "raw": text }))
                    }),
                }
            }
            Err(e) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("https_jsonrpc_error: {e}")),
                meta: serde_json::json!({}),
            },
        }
    }
}
