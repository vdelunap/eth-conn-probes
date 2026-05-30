use crate::model;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

pub struct TlsHandshakeProbe {
    pub host: String,
    pub port: u16,
}

fn make_connector() -> TlsConnector {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("ring supports safe protocol versions")
    .with_root_certificates(root_store)
    .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

#[async_trait::async_trait]
impl super::ProbeFn for TlsHandshakeProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let host = self.host.clone();
        let port = self.port;

        let fut = async move {
            let addr = format!("{host}:{port}");
            let stream = TcpStream::connect(&addr)
                .await
                .map_err(|e| anyhow::anyhow!("tcp: {e}"))?;

            let connector = make_connector();
            let server_name = rustls::pki_types::ServerName::try_from(host.as_str())
                .map_err(|e| anyhow::anyhow!("invalid SNI: {e}"))?
                .to_owned();

            let tls = connector
                .connect(server_name, stream)
                .await
                .map_err(|e| anyhow::anyhow!("tls: {e}"))?;

            let (_, conn) = tls.get_ref();

            let tls_version = conn
                .protocol_version()
                .map(|v| format!("{v:?}"))
                .unwrap_or_else(|| "unknown".into());

            let cipher = conn
                .negotiated_cipher_suite()
                .map(|c| format!("{:?}", c.suite()))
                .unwrap_or_else(|| "unknown".into());

            let cert_info = conn
                .peer_certificates()
                .and_then(|certs| certs.first())
                .and_then(|der| {
                    use x509_parser::prelude::*;
                    X509Certificate::from_der(der.as_ref())
                        .ok()
                        .map(|(_, cert)| {
                            serde_json::json!({
                                "subject": cert.subject().to_string(),
                                "issuer":  cert.issuer().to_string(),
                                "not_after": cert.validity().not_after.to_string(),
                            })
                        })
                })
                .unwrap_or(serde_json::Value::Null);

            Ok::<serde_json::Value, anyhow::Error>(serde_json::json!({
                "tls_version": tls_version,
                "cipher":      cipher,
                "cert":        cert_info,
            }))
        };

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), fut).await {
            Ok(Ok(meta)) => model::AttemptResult {
                ok: true,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: None,
                meta,
            },
            Ok(Err(e)) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(e.to_string()),
                meta: serde_json::json!({}),
            },
            Err(_) => model::AttemptResult {
                ok: false,
                rtt_ms: None,
                error: Some("tls handshake timed out".into()),
                meta: serde_json::json!({"category": "timeout"}),
            },
        }
    }
}
