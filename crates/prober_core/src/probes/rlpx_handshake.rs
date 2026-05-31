// RLPx ECIES auth handshake over TCP:30303, per EIP-8.
//
// Connect to the enode's IP:port, build auth-body
// RLP([sig(65), eph_pubkey(64), nonce(32), version=4]), ECIES-encrypt it under the
// remote's pubkey, prefix uint16_BE(len), send, then wait for anything back.
//
// Any reaction counts as success: auth-ack, disconnect, FIN or RST. Boot nodes always
// reject our throwaway identity, but a reply means the packet reached them. Only a
// timeout is a real failure: that is what DPI filtering of RLPx looks like on a port
// that is otherwise open.
use crate::{model, rlp};
use aes::Aes128;
use cipher::{KeyIvInit, StreamCipher};
use ctr::Ctr128BE;
use hmac::{Hmac, Mac};
use k256::{ecdh::EphemeralSecret, ecdsa::SigningKey, elliptic_curve::sec1::ToEncodedPoint};
use rand::{rngs::OsRng, RngCore};
use sha2::Sha256;
use sha3::{Digest, Keccak256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct RlpxHandshakeProbe {
    pub enode: String,
}

fn parse_enode(enode: &str) -> anyhow::Result<([u8; 64], String, u16)> {
    let rest = enode
        .strip_prefix("enode://")
        .ok_or_else(|| anyhow::anyhow!("not an enode:// URL"))?;
    let at = rest
        .find('@')
        .ok_or_else(|| anyhow::anyhow!("missing @ in enode URL"))?;
    let pubkey_hex = &rest[..at];
    let addr = &rest[at + 1..];

    if pubkey_hex.len() != 128 {
        anyhow::bail!(
            "enode pubkey must be 128 hex chars, got {}",
            pubkey_hex.len()
        );
    }
    let mut pubkey = [0u8; 64];
    for (i, b) in pubkey.iter_mut().enumerate() {
        *b = u8::from_str_radix(&pubkey_hex[i * 2..i * 2 + 2], 16)
            .map_err(|e| anyhow::anyhow!("invalid pubkey hex at byte {i}: {e}"))?;
    }

    let colon = addr
        .rfind(':')
        .ok_or_else(|| anyhow::anyhow!("missing port in enode URL"))?;
    let host = addr[..colon].to_string();
    let port: u16 = addr[colon + 1..]
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid port: {e}"))?;

    Ok((pubkey, host, port))
}

// ECIES_AES128_SHA256, matching go-ethereum's crypto/ecies:
//   K          = SHA256([0,0,0,1] || ECDH_shared_x)
//   enc_key    = K[0:16]
//   mac_key    = SHA256(K[16:32])        (geth hashes the mac half a second time)
//   ciphertext = AES-128-CTR(enc_key, IV, plaintext)
//   MAC        = HMAC-SHA256(mac_key, IV || ciphertext || s2)
//   output     = ecies_pubkey(65) || IV(16) || ciphertext || MAC(32)
fn ecies_encrypt(remote_pubkey: &[u8; 64], plaintext: &[u8], s2: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut rng = OsRng;

    let mut remote_full = [0u8; 65];
    remote_full[0] = 0x04;
    remote_full[1..].copy_from_slice(remote_pubkey);
    let remote_pk = k256::PublicKey::from_sec1_bytes(&remote_full)
        .map_err(|e| anyhow::anyhow!("invalid remote pubkey: {e}"))?;

    let ecies_secret = EphemeralSecret::random(&mut rng);
    let ecies_pk_encoded = ecies_secret.public_key().to_encoded_point(false);
    let ecies_pk_bytes = ecies_pk_encoded.as_bytes(); // 65 bytes

    let shared = ecies_secret.diffie_hellman(&remote_pk);
    let z = shared.raw_secret_bytes(); // x-coordinate, 32 bytes

    let mut kdf_in = [0u8; 36];
    kdf_in[3] = 1;
    kdf_in[4..].copy_from_slice(z.as_slice());
    let k = Sha256::digest(kdf_in);
    let enc_key = &k[..16];
    let mac_key = Sha256::digest(&k[16..]);

    let mut iv = [0u8; 16];
    rng.fill_bytes(&mut iv);

    let mut ciphertext = plaintext.to_vec();
    Ctr128BE::<Aes128>::new_from_slices(enc_key, &iv)
        .map_err(|e| anyhow::anyhow!("aes-ctr init: {e}"))?
        .apply_keystream(&mut ciphertext);

    let mut mac =
        Hmac::<Sha256>::new_from_slice(&mac_key).map_err(|e| anyhow::anyhow!("hmac init: {e}"))?;
    mac.update(&iv);
    mac.update(&ciphertext);
    mac.update(s2);
    let mac_tag = mac.finalize().into_bytes();

    let mut out = Vec::with_capacity(65 + 16 + ciphertext.len() + 32);
    out.extend_from_slice(ecies_pk_bytes);
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext);
    out.extend_from_slice(&mac_tag);
    Ok(out)
}

fn build_auth_packet(remote_pubkey: &[u8; 64]) -> anyhow::Result<Vec<u8>> {
    let mut rng = OsRng;

    let mut remote_full = [0u8; 65];
    remote_full[0] = 0x04;
    remote_full[1..].copy_from_slice(remote_pubkey);
    let remote_pk = k256::PublicKey::from_sec1_bytes(&remote_full)
        .map_err(|e| anyhow::anyhow!("invalid remote pubkey: {e}"))?;

    // A real node would use its persistent identity key here; a fresh one per probe is
    // fine, since the remote can't reject us on the static pubkey alone.
    let our_static = EphemeralSecret::random(&mut rng);
    let static_shared = our_static.diffie_hellman(&remote_pk);
    let static_shared_bytes = static_shared.raw_secret_bytes();

    let mut nonce = [0u8; 32];
    rng.fill_bytes(&mut nonce);

    let eph_key = SigningKey::random(&mut rng);
    let eph_pk_encoded = eph_key.verifying_key().to_encoded_point(false);
    let eph_pk_raw = &eph_pk_encoded.as_bytes()[1..]; // drop the 0x04 prefix

    // sig = ECDSA_recoverable(keccak256(static_shared XOR nonce), ephemeral_key)
    let mut xor_buf = [0u8; 32];
    for i in 0..32 {
        xor_buf[i] = static_shared_bytes[i] ^ nonce[i];
    }
    let sig_hash: [u8; 32] = Keccak256::digest(xor_buf).into();
    let (sig, rec_id) = eph_key
        .sign_prehash_recoverable(&sig_hash)
        .map_err(|e| anyhow::anyhow!("sign_prehash: {e}"))?;
    let mut sig_bytes = [0u8; 65];
    sig_bytes[..64].copy_from_slice(&sig.to_bytes());
    sig_bytes[64] = rec_id.to_byte();

    let auth_body = rlp::rlp_list(&[
        rlp::rlp_bytes(&sig_bytes),
        rlp::rlp_bytes(eph_pk_raw),
        rlp::rlp_bytes(&nonce),
        rlp::rlp_uint(4),
    ]);

    // The ciphertext length is known up front: pubkey(65) + IV(16) + body + MAC(32).
    // EIP-8 wants it both as the size prefix and as ECIES shared_info2.
    let ecies_len = 65 + 16 + auth_body.len() + 32;
    let auth_size_be = (ecies_len as u16).to_be_bytes();

    let ecies_ct = ecies_encrypt(remote_pubkey, &auth_body, &auth_size_be)?;

    let mut packet = Vec::with_capacity(2 + ecies_ct.len());
    packet.extend_from_slice(&auth_size_be);
    packet.extend_from_slice(&ecies_ct);
    Ok(packet)
}

#[async_trait::async_trait]
impl super::ProbeFn for RlpxHandshakeProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let enode = self.enode.clone();

        let fut = async move {
            let (remote_pubkey, host, port) =
                parse_enode(&enode).map_err(|e| anyhow::anyhow!("parse_enode: {e}"))?;

            let auth_packet = build_auth_packet(&remote_pubkey)
                .map_err(|e| anyhow::anyhow!("build_auth: {e}"))?;

            let addr = format!("{host}:{port}");
            let mut stream = TcpStream::connect(&addr)
                .await
                .map_err(|e| anyhow::anyhow!("tcp_connect: {e}"))?;

            stream
                .write_all(&auth_packet)
                .await
                .map_err(|e| anyhow::anyhow!("write_auth: {e}"))?;

            // read(), not read_exact(): EOF and RST count too, they still prove the
            // auth packet arrived.
            let mut buf = [0u8; 256];
            let response = match stream.read(&mut buf).await {
                Ok(0) => "remote_closed", // FIN
                Ok(_) => "ack_received",  // auth-ack or disconnect
                Err(_) => "remote_reset", // RST
            };

            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                "auth_sent_bytes": auth_packet.len(),
                "response": response,
            }))
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(meta)) => model::AttemptResult {
                ok: true,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: None,
                meta,
            },
            Ok(Err(e)) => {
                let msg = e.to_string();
                let category = if msg.starts_with("tcp_connect") {
                    "tcp_blocked"
                } else {
                    "network"
                };
                model::AttemptResult {
                    ok: false,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: Some(msg),
                    meta: serde_json::json!({"category": category}),
                }
            }
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("rlpx handshake timed out; auth packet may be DPI-filtered".into()),
                meta: serde_json::json!({"category": "timeout"}),
            },
        }
    }
}
