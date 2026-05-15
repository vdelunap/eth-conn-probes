use crate::model;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

pub struct WssSubscribeProbe {
    pub url: String,
}

#[async_trait::async_trait]
impl super::ProbeFn for WssSubscribeProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let url_str = self.url.clone();

        if let Err(e) = url::Url::parse(&url_str) {
            return model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some(format!("invalid URL: {e}")),
                meta: serde_json::json!({"category": "internal"}),
            };
        }

        let fut = async {
            let (mut ws, _) = tokio_tungstenite::connect_async(url_str)
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    let cat = if msg.contains("refused") {
                        "network"
                    } else if msg.contains("dns") || msg.contains("resolve") {
                        "dns_error"
                    } else if msg.contains("403") || msg.contains("401") {
                        "auth_required"
                    } else if msg.contains("429") {
                        "rate_limited"
                    } else {
                        "network"
                    };
                    anyhow::anyhow!("[{cat}] {msg}")
                })?;

            // Send eth_subscribe for newHeads.
            let sub_req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "eth_subscribe",
                "params": ["newHeads"]
            });
            ws.send(Message::Text(sub_req.to_string().into())).await?;

            // Wait for the subscription confirmation.
            // Success = result field contains the subscription ID string.
            // We do NOT wait for a block notification (block time ~12s > typical timeout).
            while let Some(msg) = ws.next().await {
                let msg = msg?;
                if let Message::Text(txt) = msg {
                    let v: serde_json::Value = serde_json::from_str(&txt)?;
                    let sub_id = v.get("result").and_then(|r| r.as_str()).map(str::to_string);
                    return Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                        "subscription_id": sub_id,
                        "confirmed": sub_id.is_some(),
                    }));
                }
            }
            anyhow::bail!("[network] WebSocket closed without subscription confirmation");
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(meta)) => {
                let confirmed = meta["confirmed"].as_bool().unwrap_or(false);
                model::AttemptResult {
                    ok: confirmed,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: if confirmed { None } else { Some("subscription ID not returned".into()) },
                    meta,
                }
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                let (category, error) = if let Some(rest) = msg.strip_prefix('[') {
                    if let Some(end) = rest.find(']') {
                        (rest[..end].to_string(), rest[end + 2..].trim().to_string())
                    } else {
                        ("network".into(), msg)
                    }
                } else {
                    ("network".into(), msg)
                };
                model::AttemptResult {
                    ok: false,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: Some(error),
                    meta: serde_json::json!({"category": category}),
                }
            }
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("timed out waiting for subscription confirmation".into()),
                meta: serde_json::json!({"category": "timeout"}),
            },
        }
    }
}
