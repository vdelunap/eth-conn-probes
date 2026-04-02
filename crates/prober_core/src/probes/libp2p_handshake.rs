use crate::model;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct LibP2pHandshakeProbe {
    pub host: String,
    pub port: u16,
}

// multistream-select wire format: varint(len) + payload (no null terminator).
// "/multistream/1.0.0\n" is 19 bytes → length-prefix byte 0x13.
const MS_HEADER: &[u8] = b"\x13/multistream/1.0.0\n";
// "/noise\n" is 7 bytes → length-prefix byte 0x07.
const MS_NOISE: &[u8] = b"\x07/noise\n";

#[async_trait::async_trait]
impl super::ProbeFn for LibP2pHandshakeProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        let result = tokio::time::timeout(timeout, async {
            let addr = format!("{}:{}", self.host, self.port);
            let mut stream = TcpStream::connect(&addr).await?;

            // Step 1: send the multistream-select header.
            stream.write_all(MS_HEADER).await?;

            // Step 2: read the echoed header back (same bytes if the peer speaks multistream).
            let mut buf = vec![0u8; MS_HEADER.len()];
            stream.read_exact(&mut buf).await?;

            if buf != MS_HEADER {
                return Err(anyhow::anyhow!(
                    "unexpected_header: {:?}",
                    String::from_utf8_lossy(&buf)
                ));
            }

            // Step 3: propose /noise as the security protocol.
            stream.write_all(MS_NOISE).await?;

            // Step 4: read the response — same bytes mean /noise was accepted.
            let mut resp = vec![0u8; MS_NOISE.len()];
            stream.read_exact(&mut resp).await?;

            let noise_accepted = resp == MS_NOISE;
            Ok::<_, anyhow::Error>(noise_accepted)
        })
        .await;

        let rtt_ms = model::now_ms().saturating_sub(started);

        match result {
            Ok(Ok(noise_accepted)) => model::AttemptResult {
                ok: true,
                rtt_ms: Some(rtt_ms),
                error: None,
                meta: serde_json::json!({ "noise_accepted": noise_accepted }),
            },
            Ok(Err(e)) => {
                let msg = e.to_string();
                let category = if msg.contains("unexpected_header") {
                    "api_error"
                } else {
                    "network"
                };
                model::AttemptResult {
                    ok: false,
                    rtt_ms: Some(rtt_ms),
                    error: Some(msg),
                    meta: serde_json::json!({ "category": category }),
                }
            }
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("timeout".to_string()),
                meta: serde_json::json!({ "category": "timeout" }),
            },
        }
    }
}
