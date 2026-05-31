use crate::{config, model};
use std::sync::Arc;

pub mod beacon_https;
pub mod beacon_peers;
pub mod dns;
pub mod dns_compare;
pub mod http_control;
pub mod https_jsonrpc;
pub mod https_jsonrpc_write;
pub mod libp2p_handshake;
pub mod tcp;
pub mod tls_handshake;
pub mod wss_jsonrpc;
pub mod wss_subscribe;

#[cfg(feature = "discv4")]
pub mod discv4_ping;

#[cfg(feature = "discv5")]
pub mod discv5_ping;

#[cfg(feature = "rlpx")]
pub mod rlpx_handshake;

#[derive(Clone)]
pub struct ProbeJob {
    pub kind: model::ProbeKind,
    pub target_label: String,
    pub run: Arc<dyn ProbeFn + Send + Sync>,
}

impl std::fmt::Debug for ProbeJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeJob")
            .field("kind", &self.kind)
            .field("target_label", &self.target_label)
            .finish()
    }
}

#[async_trait::async_trait]
pub trait ProbeFn {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult;
}

pub fn build_jobs(cfg: &config::Config) -> anyhow::Result<Vec<ProbeJob>> {
    let mut jobs: Vec<ProbeJob> = Vec::new();

    // ---- RPC provider availability ----

    if cfg.probes.control_http.enabled {
        let url = cfg.probes.control_http.url.clone();
        let expect = cfg.probes.control_http.expect_body.clone();
        jobs.push(ProbeJob {
            kind: model::ProbeKind::HttpControl,
            target_label: url.clone(),
            run: Arc::new(http_control::HttpControlProbe {
                url,
                expect_body: expect,
            }),
        });
    }

    for t in &cfg.probes.tcp {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::DnsResolve,
            target_label: format!("{} ({})", t.host, t.name),
            run: Arc::new(dns::DnsResolveProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });

        jobs.push(ProbeJob {
            kind: model::ProbeKind::TcpConnect,
            target_label: format!("{}:{} ({})", t.host, t.port, t.name),
            run: Arc::new(tcp::TcpConnectProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });

        jobs.push(ProbeJob {
            kind: model::ProbeKind::TlsHandshake,
            target_label: format!("{}:{} ({})", t.host, t.port, t.name),
            run: Arc::new(tls_handshake::TlsHandshakeProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });
    }

    for t in &cfg.probes.https_jsonrpc {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::HttpsJsonRpc,
            target_label: format!("{} ({})", t.url, t.name),
            run: Arc::new(https_jsonrpc::HttpsJsonRpcProbe {
                url: t.url.clone(),
                method: t.method.clone(),
            }),
        });

        jobs.push(ProbeJob {
            kind: model::ProbeKind::HttpsJsonRpcWrite,
            target_label: format!("{} ({})", t.url, t.name),
            run: Arc::new(https_jsonrpc_write::HttpsJsonRpcWriteProbe { url: t.url.clone() }),
        });
    }

    for t in &cfg.probes.wss_jsonrpc {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::WssJsonRpc,
            target_label: format!("{} ({})", t.url, t.name),
            run: Arc::new(wss_jsonrpc::WssJsonRpcProbe {
                url: t.url.clone(),
                method: t.method.clone(),
            }),
        });

        jobs.push(ProbeJob {
            kind: model::ProbeKind::WssSubscribe,
            target_label: format!("{} ({})", t.url, t.name),
            run: Arc::new(wss_subscribe::WssSubscribeProbe { url: t.url.clone() }),
        });
    }

    // Empty by default: execution nodes advertise enode://, not enr:-.
    #[cfg(feature = "discv5")]
    for t in &cfg.probes.discv5_execution {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::Discv5Ping,
            target_label: format!("{} ({})", t.enr, t.name),
            run: Arc::new(discv5_ping::Discv5PingProbe { enr: t.enr.clone() }),
        });
    }

    // ---- Execution layer P2P ----

    for t in &cfg.probes.p2p_boot_nodes {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::P2pTcpConnect,
            target_label: format!("{}:{} ({})", t.host, t.port, t.name),
            run: Arc::new(tcp::TcpConnectProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });
    }

    #[cfg(feature = "discv4")]
    for t in &cfg.probes.discv4_execution {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::Discv4Ping,
            target_label: format!("{}:{} ({})", t.host, t.port, t.name),
            run: Arc::new(discv4_ping::Discv4PingProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });
    }

    #[cfg(feature = "rlpx")]
    for t in &cfg.probes.rlpx_targets {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::RlpxHandshake,
            target_label: format!("{} ({})", t.enode, t.name),
            run: Arc::new(rlpx_handshake::RlpxHandshakeProbe {
                enode: t.enode.clone(),
            }),
        });
    }

    for t in &cfg.probes.dns_compare {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::DnsCompare,
            target_label: format!("{} ({})", t.host, t.name),
            run: Arc::new(dns_compare::DnsCompareProbe {
                host: t.host.clone(),
            }),
        });
    }

    // ---- Consensus layer (Beacon chain) ----

    // Only pings: consensus boot nodes are discovery-only and block inbound TCP:9000.
    // The beacon TCP and libp2p jobs come from build_live_peer_jobs instead.
    #[cfg(feature = "discv5")]
    for t in &cfg.probes.discv5_consensus {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::BeaconDiscv5Ping,
            target_label: format!("{} ({})", t.enr, t.name),
            run: Arc::new(discv5_ping::Discv5PingProbe { enr: t.enr.clone() }),
        });
    }

    for t in &cfg.probes.beacon_https {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::BeaconHttps,
            target_label: format!("{} ({})", t.url, t.name),
            run: Arc::new(beacon_https::BeaconHttpsProbe { url: t.url.clone() }),
        });
    }

    Ok(jobs)
}

pub async fn run_jobs(run_cfg: &config::RunConfig, jobs: Vec<ProbeJob>) -> Vec<model::ProbeRun> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(run_cfg.parallelism));
    // (kind, target) travel next to the handle so a panicked task can still be reported.
    let mut handles: Vec<(
        model::ProbeKind,
        String,
        tokio::task::JoinHandle<model::ProbeRun>,
    )> = Vec::new();

    for job in jobs {
        let kind = job.kind.clone();
        let target = job.target_label.clone();
        let sem = semaphore.clone();
        let attempts = run_cfg.attempts;
        let min_successes = run_cfg.min_successes;
        let timeout_ms = run_cfg.timeout_ms;

        handles.push((
            kind,
            target,
            tokio::spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore");
                let mut attempt_results = Vec::new();

                for _ in 0..attempts {
                    let r = job.run.run(timeout_ms).await;
                    attempt_results.push(r);
                }

                let summary = model::summarize_attempts(&attempt_results, min_successes);

                model::ProbeRun {
                    kind: job.kind,
                    target: job.target_label,
                    attempts: attempt_results,
                    summary,
                }
            }),
        ));
    }

    let mut out = Vec::new();
    for (kind, target, h) in handles {
        match h.await {
            Ok(r) => out.push(r),
            Err(e) => {
                // Panicked or cancelled: report it as a failure rather than dropping it.
                tracing::error!("probe task panicked: kind={kind:?} target={target} err={e}");
                out.push(model::ProbeRun {
                    kind,
                    target,
                    attempts: vec![model::AttemptResult {
                        ok: false,
                        rtt_ms: None,
                        error: Some(format!("probe task panicked: {e}")),
                        meta: serde_json::json!({"category": "internal"}),
                    }],
                    summary: model::ProbeSummary {
                        success_count: 0,
                        failure_count: 1,
                        min_rtt_ms: None,
                        avg_rtt_ms: None,
                        max_rtt_ms: None,
                        ok: false,
                    },
                });
            }
        }
    }
    out
}

/// One TCP job and one libp2p job per live peer.
pub fn build_live_peer_jobs(peers: Vec<beacon_peers::LivePeer>) -> Vec<ProbeJob> {
    let mut jobs = Vec::with_capacity(peers.len() * 2);
    for peer in peers {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::BeaconTcpConnect,
            target_label: peer.label.clone(),
            run: Arc::new(tcp::TcpConnectProbe {
                host: peer.host.clone(),
                port: peer.port,
            }),
        });
        jobs.push(ProbeJob {
            kind: model::ProbeKind::LibP2pHandshake,
            target_label: peer.label.clone(),
            run: Arc::new(libp2p_handshake::LibP2pHandshakeProbe {
                host: peer.host,
                port: peer.port,
            }),
        });
    }
    jobs
}

#[cfg(test)]
#[cfg(feature = "discv5")]
mod tests {
    /// Dumps the ip/tcp/udp fields of the consensus boot node ENRs. Not an assertion,
    /// just a way to see what a boot node actually advertises:
    ///   cargo test -p prober_core --features discv5 -- enr_decode --nocapture
    #[test]
    fn enr_decode() {
        let enrs = [
            ("teku-ohio",        "enr:-Iu4QLm7bZGdAt9NSeJG0cEnJohWcQTQaI9wFLu3Q7eHIDfrI4cwtzvEW3F3VbG9XdFXlrHyFGeXPn9snTCQJ9bnMRABgmlkgnY0gmlwhAOTJQCJc2VjcDI1NmsxoQIZdZD6tDYpkpEfVo5bgiU8MGRjhcOmHGD2nErK0UKRrIN0Y3CCIyiDdWRwgiMo"),
            ("teku-sydney",      "enr:-Iu4QEDJ4Wa_UQNbK8Ay1hFEkXvd8psolVK6OhfTL9irqz3nbXxxWyKwEplPfkju4zduVQj6mMhUCm9R2Lc4YM5jPcIBgmlkgnY0gmlwhANrfESJc2VjcDI1NmsxoQJCYz2-nsqFpeEj6eov9HSi9QssIVIVNr0I89J1vXM9foN0Y3CCIyiDdWRwgiMo"),
            ("nimbus-frankfurt", "enr:-LK4QA8FfhaAjlb_BXsXxSfiysR7R52Nhi9JBt4F8SPssu8hdE1BXQQEtVDC3qStCW60LSO7hEsVHv5zm8_6Vnjhcn0Bh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhAN4aBKJc2VjcDI1NmsxoQJerDhsJ-KxZ8sHySMOCmTO6sHM3iCFQ6VMvLTe948MyYN0Y3CCI4yDdWRwgiOM"),
        ];

        for (name, enr_str) in &enrs {
            let result = enr_str.parse::<discv5::enr::Enr<discv5::enr::CombinedKey>>();
            match result {
                Err(e) => println!("{name}: PARSE FAILED: {e}"),
                Ok(enr) => {
                    println!(
                        "{name}: ip4={:?}  ip6={:?}  tcp4={:?}  tcp6={:?}  udp4={:?}  udp6={:?}",
                        enr.ip4(),
                        enr.ip6(),
                        enr.tcp4(),
                        enr.tcp6(),
                        enr.udp4(),
                        enr.udp6(),
                    );
                }
            }
        }
    }
}
