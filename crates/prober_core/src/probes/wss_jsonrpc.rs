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
                error: Some(format!("bad_url: {e}")),
                meta: serde_json::json!({}),
            };
        }

        let fut = async {
            let (mut ws, _resp) = tokio_tungstenite::connect_async(url_str).await?;
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

            anyhow::bail!("ws_closed_without_response");
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(v)) => {
                let ok = v.get("result").is_some();
                model::AttemptResult {
                    ok,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: if ok {
                        None
                    } else {
                        Some("bad_jsonrpc_response".to_string())
                    },
                    meta: serde_json::json!({ "response": v }),
                }
            }
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("wss_error: {e}")),
                meta: serde_json::json!({}),
            },
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("timeout".to_string()),
                meta: serde_json::json!({}),
            },
        }
    }
}
