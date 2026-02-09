use crate::model;

pub struct Discv5PingProbe {
    pub enr: String,
}

#[cfg(feature = "discv5")]
#[async_trait::async_trait]
impl super::ProbeFn for Discv5PingProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        use discv5::socket::ListenConfig;
        use discv5::{enr, ConfigBuilder, Discv5, TokioExecutor};
        use std::net::Ipv4Addr;

        let started = model::now_ms();

        let remote: enr::Enr<enr::CombinedKey> = match self.enr.parse() {
            Ok(x) => x,
            Err(e) => {
                return model::AttemptResult {
                    ok: false,
                    rtt_ms: None,
                    error: Some(format!("bad_enr: {e}")),
                    meta: serde_json::json!({}),
                };
            }
        };

        let enr_key = enr::CombinedKey::generate_secp256k1();
        let local_enr = enr::Enr::empty(&enr_key).expect("local enr");

        let listen_config = ListenConfig::Ipv4 {
            ip: Ipv4Addr::UNSPECIFIED,
            port: 0,
        };

        let config = ConfigBuilder::new(listen_config)
            .executor(Box::new(TokioExecutor))
            .build();

        let mut discv5 = match Discv5::new(local_enr, enr_key, config) {
            Ok(x) => x,
            Err(e) => {
                return model::AttemptResult {
                    ok: false,
                    rtt_ms: None,
                    error: Some(format!("discv5_new_error: {e}")),
                    meta: serde_json::json!({}),
                };
            }
        };

        let run = async {
            discv5.start().await?;
            let pong = discv5.send_ping(remote).await?;
            Ok::<_, anyhow::Error>(pong)
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), run).await {
            Ok(Ok(pong)) => model::AttemptResult {
                ok: true,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: None,
                meta: serde_json::json!({ "pong": format!("{pong:?}") }),
            },
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("discv5_ping_error: {e}")),
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
