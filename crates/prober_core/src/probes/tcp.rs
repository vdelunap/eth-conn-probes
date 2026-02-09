use crate::model;
use tokio::net::TcpStream;

pub struct TcpConnectProbe {
    pub host: String,
    pub port: u16,
}

#[async_trait::async_trait]
impl super::ProbeFn for TcpConnectProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let addr = format!("{}:{}", self.host, self.port);

        let res = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            TcpStream::connect(addr),
        )
        .await;

        match res {
            Ok(Ok(_stream)) => model::AttemptResult {
                ok: true,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: None,
                meta: serde_json::json!({}),
            },
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("tcp_error: {e}")),
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
