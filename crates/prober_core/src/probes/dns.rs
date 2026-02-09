use crate::model;
use tokio::net::lookup_host;

pub struct DnsResolveProbe {
    pub host: String,
    pub port: u16,
}

#[async_trait::async_trait]
impl super::ProbeFn for DnsResolveProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let addr = format!("{}:{}", self.host, self.port);

        let res = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            lookup_host(addr),
        )
        .await;

        match res {
            Ok(Ok(iter)) => {
                let addrs: Vec<String> = iter.map(|a| a.to_string()).collect();
                model::AttemptResult {
                    ok: !addrs.is_empty(),
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: None,
                    meta: serde_json::json!({ "addrs": addrs }),
                }
            }
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("dns_error: {e}")),
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
