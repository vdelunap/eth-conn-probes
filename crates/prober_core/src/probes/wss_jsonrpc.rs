use crate::model;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

pub struct WssJsonRpcProbe {
    pub url: String,
    pub method: String,
}

#[async_trait::async_trait]
impl super::ProbeFn for WssJsonRpcProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let url_str = self.url.clone();

        if let Err(e) = url::Url::parse(&url_str) {
            return model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some(format!("invalid URL: {e}")),
                meta: serde_json::json!({ "category": "internal" }),
            };
        }

        let fut = async {
            let (mut ws, _resp) = tokio_tungstenite::connect_async(url_str)
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    let category = if msg.contains("refused") {
                        "network"
                    } else if msg.contains("dns")
                        || msg.contains("resolve")
                        || msg.contains("lookup")
                    {
                        "dns_error"
                    } else if msg.contains("403") || msg.contains("401") {
                        "auth_required"
                    } else if msg.contains("429") {
                        "rate_limited"
                    } else {
                        "network"
                    };
                    // Tagged so the outer match can recover the category.
                    anyhow::anyhow!("[{category}] {msg}")
                })?;

            let payload = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": self.method,
                "params": []
            });
            ws.send(Message::Text(payload.to_string().into())).await?;

            while let Some(msg) = ws.next().await {
                let msg = msg?;
                if let Message::Text(txt) = msg {
                    let v: serde_json::Value = serde_json::from_str(&txt)?;
                    return Ok::<serde_json::Value, anyhow::Error>(v);
                }
            }
            anyhow::bail!("[network] WebSocket closed without a response");
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(v)) => {
                let (ok, error, category) = if v.get("result").is_some() {
                    (true, None, "ok")
                } else if v.get("error").is_some() {
                    let err_obj = &v["error"];
                    let msg = err_obj["message"].as_str().unwrap_or("unknown error");
                    let code = err_obj["code"].as_i64().unwrap_or(0);
                    (false, Some(format!("RPC error {code}: {msg}")), "rpc_error")
                } else {
                    (false, Some("unexpected response".to_string()), "api_error")
                };
                model::AttemptResult {
                    ok,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error,
                    meta: serde_json::json!({ "category": category, "response": v }),
                }
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                let (category, error) = if let Some(rest) = msg.strip_prefix('[') {
                    if let Some(end) = rest.find(']') {
                        let cat = &rest[..end];
                        let detail = rest[end + 2..].trim().to_string();
                        (cat.to_string(), detail)
                    } else {
                        ("network".to_string(), msg)
                    }
                } else {
                    ("network".to_string(), msg)
                };
                model::AttemptResult {
                    ok: false,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: Some(error),
                    meta: serde_json::json!({ "category": category }),
                }
            }
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("connection timed out".to_string()),
                meta: serde_json::json!({ "category": "timeout" }),
            },
        }
    }
}
