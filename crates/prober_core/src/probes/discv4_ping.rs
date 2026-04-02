/// DiscV4 ping probe for Ethereum execution-layer boot nodes.
///
/// The execution P2P discovery protocol (devp2p discv4) runs over UDP on port 30303.
/// This is distinct from the discv5 protocol used by consensus nodes.
///
/// Packet format (from the devp2p spec):
///   packet = hash(32) || signature(65) || packet-type(1) || RLP-data
///   hash      = keccak256(signature || packet-type || RLP-data)
///   signature = sign(keccak256(packet-type || RLP-data))   [65 bytes: 64 sig + 1 rec-id]
///
/// Ping RLP-data: [version=4, from=[ip,udp,tcp], to=[ip,udp,tcp], expiration]
/// Pong RLP-data: [to, ping-hash, expiration]
///
/// Flow: send PING → expect PONG (packet-type 0x02 at offset 97).
/// Some nodes do an "endpoint proof" (they send a PING back before PONGing us);
/// this probe handles that by responding with a PONG before re-waiting.
use crate::model;
use sha3::{Digest, Keccak256};

const PACKET_PING: u8 = 0x01;
const PACKET_PONG: u8 = 0x02;
/// Minimum packet length: 32 (hash) + 65 (sig) + 1 (type)
const HDR_LEN: usize = 98;

pub struct Discv4PingProbe {
    pub host: String,
    pub port: u16,
}

// ---------------------------------------------------------------------------
// Minimal RLP encoding (only what we need for ping/pong packets)
// ---------------------------------------------------------------------------

fn rlp_bytes(data: &[u8]) -> Vec<u8> {
    if data.len() == 1 && data[0] < 0x80 {
        vec![data[0]]
    } else if data.len() <= 55 {
        let mut out = vec![0x80 + data.len() as u8];
        out.extend_from_slice(data);
        out
    } else {
        let len_enc = be_bytes(data.len() as u64);
        let mut out = vec![0xb7 + len_enc.len() as u8];
        out.extend_from_slice(&len_enc);
        out.extend_from_slice(data);
        out
    }
}

fn rlp_uint(n: u64) -> Vec<u8> {
    if n == 0 {
        vec![0x80]
    } else {
        let b = n.to_be_bytes();
        let start = b.iter().position(|&x| x != 0).unwrap_or(7);
        rlp_bytes(&b[start..])
    }
}

fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = items.iter().flat_map(|i| i.iter().copied()).collect();
    if payload.len() <= 55 {
        let mut out = vec![0xc0 + payload.len() as u8];
        out.extend_from_slice(&payload);
        out
    } else {
        let len_enc = be_bytes(payload.len() as u64);
        let mut out = vec![0xf7 + len_enc.len() as u8];
        out.extend_from_slice(&len_enc);
        out.extend_from_slice(&payload);
        out
    }
}

fn be_bytes(n: u64) -> Vec<u8> {
    let b = n.to_be_bytes();
    let start = b.iter().position(|&x| x != 0).unwrap_or(7);
    b[start..].to_vec()
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(data);
    h.finalize().into()
}

// ---------------------------------------------------------------------------
// Packet builders
// ---------------------------------------------------------------------------

fn sign_packet(key: &k256::ecdsa::SigningKey, pkt_type: u8, rlp_data: &[u8]) -> Vec<u8> {
    // signature = sign(keccak256(packet_type || rlp_data))
    let mut to_sign = vec![pkt_type];
    to_sign.extend_from_slice(rlp_data);
    let sig_hash = keccak256(&to_sign);

    let (sig, rec_id) = key
        .sign_prehash_recoverable(&sig_hash)
        .expect("discv4 signing failed");

    let mut signature = [0u8; 65];
    signature[..64].copy_from_slice(&sig.to_bytes());
    signature[64] = rec_id.to_byte();

    // hash = keccak256(signature || packet_type || rlp_data)
    let mut to_hash = Vec::with_capacity(65 + 1 + rlp_data.len());
    to_hash.extend_from_slice(&signature);
    to_hash.push(pkt_type);
    to_hash.extend_from_slice(rlp_data);
    let hash = keccak256(&to_hash);

    let mut packet = Vec::with_capacity(HDR_LEN + rlp_data.len());
    packet.extend_from_slice(&hash);
    packet.extend_from_slice(&signature);
    packet.push(pkt_type);
    packet.extend_from_slice(rlp_data);
    packet
}

fn build_ping(key: &k256::ecdsa::SigningKey, target_ip: [u8; 4], target_port: u16) -> Vec<u8> {
    let expiration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        + 20;

    let rlp_data = rlp_list(&[
        rlp_uint(4), // version
        rlp_list(&[
            rlp_bytes(&[0u8, 0, 0, 0]),
            rlp_uint(0),
            rlp_uint(0),
        ]),
        rlp_list(&[
            rlp_bytes(&target_ip),
            rlp_uint(target_port as u64),
            rlp_uint(target_port as u64),
        ]),
        rlp_uint(expiration),
    ]);

    sign_packet(key, PACKET_PING, &rlp_data)
}

fn build_pong(
    key: &k256::ecdsa::SigningKey,
    to_ip: [u8; 4],
    to_port: u16,
    ping_hash: &[u8],
    expiration: u64,
) -> Vec<u8> {
    let rlp_data = rlp_list(&[
        rlp_list(&[
            rlp_bytes(&to_ip),
            rlp_uint(to_port as u64),
            rlp_uint(to_port as u64),
        ]),
        rlp_bytes(ping_hash),
        rlp_uint(expiration),
    ]);
    sign_packet(key, PACKET_PONG, &rlp_data)
}

// ---------------------------------------------------------------------------
// Probe implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl super::ProbeFn for Discv4PingProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();

        // The boot nodes are raw IPs, but lookup_host handles both IPs and hostnames.
        let addr_str = format!("{}:{}", self.host, self.port);
        let target_addr = match tokio::net::lookup_host(&addr_str).await {
            Ok(mut addrs) => match addrs.find(|a| a.is_ipv4()) {
                Some(a) => a,
                None => {
                    return model::AttemptResult {
                        ok: false,
                        rtt_ms: None,
                        error: Some("dns_error: no IPv4 address resolved".to_string()),
                        meta: serde_json::json!({ "category": "dns_error" }),
                    }
                }
            },
            Err(e) => {
                return model::AttemptResult {
                    ok: false,
                    rtt_ms: None,
                    error: Some(format!("dns_error: {e}")),
                    meta: serde_json::json!({ "category": "dns_error" }),
                }
            }
        };

        let std::net::IpAddr::V4(ipv4) = target_addr.ip() else {
            unreachable!("filtered to IPv4 above")
        };

        let key = k256::ecdsa::SigningKey::random(&mut rand::thread_rng());
        let ping = build_ping(&key, ipv4.octets(), self.port);

        let socket = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(e) => {
                return model::AttemptResult {
                    ok: false,
                    rtt_ms: None,
                    error: Some(format!("socket_error: {e}")),
                    meta: serde_json::json!({ "category": "internal" }),
                }
            }
        };

        if let Err(e) = socket.send_to(&ping, target_addr).await {
            return model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("send_error: {e}")),
                meta: serde_json::json!({ "category": "network" }),
            };
        }

        // Receive loop: wait for PONG. If we receive a PING (endpoint proof), respond with PONG.
        let probe = async {
            let mut buf = vec![0u8; 1280];
            loop {
                let (n, src) = socket
                    .recv_from(&mut buf)
                    .await
                    .map_err(|e| anyhow::anyhow!("recv_error: {e}"))?;

                if n < HDR_LEN {
                    continue;
                }

                match buf[97] {
                    PACKET_PONG => return Ok::<(), anyhow::Error>(()),
                    PACKET_PING => {
                        // Endpoint proof: remote wants us to prove our IP is real.
                        // Respond with a PONG so they will PONG us back.
                        let ping_hash = &buf[..32];
                        let expiration = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                            + 20;
                        if let std::net::SocketAddr::V4(src4) = src {
                            let pong = build_pong(
                                &key,
                                src4.ip().octets(),
                                src4.port(),
                                ping_hash,
                                expiration,
                            );
                            let _ = socket.send_to(&pong, src).await;
                        }
                    }
                    _ => {} // ignore other packet types
                }
            }
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), probe).await {
            Ok(Ok(())) => model::AttemptResult {
                ok: true,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: None,
                meta: serde_json::json!({}),
            },
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("discv4_error: {e}")),
                meta: serde_json::json!({ "category": "network" }),
            },
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("timeout".to_string()),
                meta: serde_json::json!({ "category": "timeout" }),
            },
        }
    }
}
