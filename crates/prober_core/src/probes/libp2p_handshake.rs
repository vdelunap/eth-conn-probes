// multistream-select negotiation against a consensus node's libp2p port (usually 9000).
//
// We send "/multistream/1.0.0\n"; if the remote echoes it back, it is a live libp2p
// node and the probe passes. Then we propose "/noise\n". The answer is either
// "/noise\n" or "na\n", and either way the node is up. Which one it was goes in meta.
use crate::model;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct LibP2pHandshakeProbe {
    pub host: String,
    pub port: u16,
}

// Varint-prefixed, and the length counts the trailing \n.
const MS_HEADER: &[u8] = b"\x13/multistream/1.0.0\n";
const MS_NOISE: &[u8] = b"\x07/noise\n";

async fn read_ms_line(stream: &mut TcpStream) -> anyhow::Result<Vec<u8>> {
    // Every message we care about is under 128 bytes, so the varint is one byte.
    let len = stream.read_u8().await? as usize;
    if len == 0 {
        return Ok(vec![]);
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

#[async_trait::async_trait]
impl super::ProbeFn for LibP2pHandshakeProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let host = self.host.clone();
        let port = self.port;

        let fut = async move {
            let addr = format!("{host}:{port}");
            let mut stream = TcpStream::connect(&addr)
                .await
                .map_err(|e| anyhow::anyhow!("tcp_connect: {e}"))?;

            stream
                .write_all(MS_HEADER)
                .await
                .map_err(|e| anyhow::anyhow!("write_ms_header: {e}"))?;

            let remote_header = read_ms_line(&mut stream)
                .await
                .map_err(|e| anyhow::anyhow!("read_ms_header: {e}"))?;

            let is_libp2p = remote_header
                .windows(b"/multistream/1.0.0".len())
                .any(|w| w == b"/multistream/1.0.0");

            if !is_libp2p {
                return Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                    "is_libp2p": false,
                    "noise_accepted": false,
                    "remote_header": String::from_utf8_lossy(&remote_header).trim().to_string(),
                    "category": "not_libp2p",
                }));
            }

            stream
                .write_all(MS_NOISE)
                .await
                .map_err(|e| anyhow::anyhow!("write_noise_proposal: {e}"))?;

            let noise_resp = read_ms_line(&mut stream).await.unwrap_or_default();
            let noise_accepted = noise_resp.windows(b"/noise".len()).any(|w| w == b"/noise");

            Ok(serde_json::json!({
                "is_libp2p":     true,
                "noise_accepted": noise_accepted,
                "noise_response": String::from_utf8_lossy(&noise_resp).trim().to_string(),
            }))
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(meta)) => {
                let is_libp2p = meta["is_libp2p"].as_bool().unwrap_or(false);
                model::AttemptResult {
                    ok: is_libp2p,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: if is_libp2p {
                        None
                    } else {
                        Some("remote did not acknowledge /multistream/1.0.0".into())
                    },
                    meta,
                }
            }
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(e.to_string()),
                meta: serde_json::json!({"category": "network"}),
            },
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("timed out during libp2p handshake".into()),
                meta: serde_json::json!({"category": "timeout"}),
            },
        }
    }
}
