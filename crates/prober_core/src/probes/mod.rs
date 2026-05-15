use crate::{config, model};
use std::sync::Arc;

pub mod beacon_https;
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

    // ---- Section 1: RPC provider availability ----

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

    // Each TCP target generates: DNS resolve + TCP connect + TLS handshake.
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

    // Each https_jsonrpc target generates: JSON-RPC read + JSON-RPC write probes.
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
            run: Arc::new(https_jsonrpc_write::HttpsJsonRpcWriteProbe {
                url: t.url.clone(),
            }),
        });
    }

    // Each wss_jsonrpc target generates: WSS JSON-RPC + WSS subscribe probes.
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
            run: Arc::new(wss_subscribe::WssSubscribeProbe {
                url: t.url.clone(),
            }),
        });
    }

    // DiscV5 pings to execution layer boot nodes (ENRs on port 30303).
    // Empty by default — execution nodes advertise enode://, not enr:-.
    #[cfg(feature = "discv5")]
    for t in &cfg.probes.discv5_execution {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::Discv5Ping,
            target_label: format!("{} ({})", &t.enr, &t.name),
            run: Arc::new(discv5_ping::Discv5PingProbe { enr: t.enr.clone() }),
        });
    }

    // ---- Section 2: Ethereum execution layer P2P ----

    // TCP connect to execution boot nodes on port 30303.
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

    // DiscV4 UDP ping to execution boot nodes on port 30303.
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

    // RLPx ECIES auth handshake to execution boot nodes on port 30303.
    // Proves the RLPx transport layer is reachable (TCP open + RLPx not filtered by DPI).
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

    // DNS comparison: system resolver vs Cloudflare DoH (1.1.1.1).
    for t in &cfg.probes.dns_compare {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::DnsCompare,
            target_label: format!("{} ({})", t.host, t.name),
            run: Arc::new(dns_compare::DnsCompareProbe {
                host: t.host.clone(),
            }),
        });
    }

    // ---- Section 3: Ethereum consensus layer (Beacon chain) ----

    // DiscV5 pings to consensus boot nodes (ENRs on port 9000).
    // When discv5 is enabled, also extract ip4+tcp4 from each ENR to generate
    // plain TCP connect and libp2p multistream-select probes for the same host.
    #[cfg(feature = "discv5")]
    for t in &cfg.probes.discv5_consensus {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::BeaconDiscv5Ping,
            target_label: format!("{} ({})", &t.enr, &t.name),
            run: Arc::new(discv5_ping::Discv5PingProbe { enr: t.enr.clone() }),
        });

        // Derive TCP and libp2p probes from ip4/tcp4 or ip6/tcp6 fields in the ENR.
        // Lighthouse boot nodes use IPv6 addresses (ip6 ENR key); ip4() returns None for them.
        match t.enr.parse::<discv5::enr::Enr<discv5::enr::CombinedKey>>() {
            Err(e) => {
                tracing::warn!(name = %t.name, error = %e, "ENR parse failed — no beacon_tcp/libp2p probes");
            }
            Ok(enr) => {
                let ip4 = enr.ip4();
                let ip6 = enr.ip6();
                let tcp4 = enr.tcp4();
                let tcp6 = enr.tcp6();
                let udp4 = enr.udp4();
                let udp6 = enr.udp6();
                tracing::debug!(
                    name = %t.name,
                    ?ip4, ?ip6, ?tcp4, ?tcp6, ?udp4, ?udp6,
                    "ENR decoded"
                );

                // Resolve (host, port) for TCP probes.
                // Prefer explicit tcp key; fall back to udp port when absent — Ethereum
                // consensus nodes conventionally use the same port for both UDP (DiscV5)
                // and TCP (libp2p), but some boot nodes omit the tcp ENR key.
                let (host, port) = if let (Some(ip), Some(p)) = (ip4, tcp4) {
                    (Some(ip.to_string()), Some(p))
                } else if let (Some(ip), Some(p)) = (ip4, udp4) {
                    (Some(ip.to_string()), Some(p))
                } else if let Some(ip) = ip6 {
                    // IPv6: bracket the address so format!("{host}:{port}") is valid for connect.
                    let p = tcp6.or(tcp4).or(udp6).or(udp4);
                    (p.map(|_| format!("[{ip}]")), p)
                } else {
                    (None, None)
                };

                if let (Some(host), Some(port)) = (host, port) {
                    jobs.push(ProbeJob {
                        kind: model::ProbeKind::BeaconTcpConnect,
                        target_label: format!("{host}:{port} ({})", t.name),
                        run: Arc::new(tcp::TcpConnectProbe {
                            host: host.clone(),
                            port,
                        }),
                    });
                    jobs.push(ProbeJob {
                        kind: model::ProbeKind::LibP2pHandshake,
                        target_label: format!("{host}:{port} ({})", t.name),
                        run: Arc::new(libp2p_handshake::LibP2pHandshakeProbe {
                            host,
                            port,
                        }),
                    });
                } else {
                    tracing::warn!(
                        name = %t.name,
                        ?ip4, ?ip6, ?tcp4, ?tcp6,
                        "ENR has no usable ip4/ip6 + tcp — no beacon_tcp/libp2p probes"
                    );
                }
            }
        }
    }

    // HTTP GET to public beacon chain REST API endpoints.
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
    // Store (kind, target) alongside the handle so panics can be surfaced.
    let mut handles: Vec<(model::ProbeKind, String, tokio::task::JoinHandle<model::ProbeRun>)> =
        Vec::new();

    for job in jobs {
        let kind = job.kind.clone();
        let target = job.target_label.clone();
        let sem = semaphore.clone();
        let attempts = run_cfg.attempts;
        let min_successes = run_cfg.min_successes;
        let timeout_ms = run_cfg.timeout_ms;

        handles.push((kind, target, tokio::spawn(async move {
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
        })));
    }

    let mut out = Vec::new();
    for (kind, target, h) in handles {
        match h.await {
            Ok(r) => out.push(r),
            Err(e) => {
                // Task panicked or was cancelled — surface it as a failed result
                // so it appears in the report instead of being silently dropped.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(feature = "discv5")]
mod tests {
    /// Decodes all candidate consensus boot node ENRs and prints every ip/tcp field.
    /// Run with:
    ///   cargo test -p prober_core --features discv5 -- enr_decode --nocapture
    #[test]
    fn enr_decode() {
        let enrs = [
            // --- existing (baseline) ---
            ("lighthouse-sydney",  "enr:-Le4QPUXJS2BTORXxyx2Ia-9ae4YqA_JWX3ssj4E_J-3z1A-HmFGrU8BpvpqhNabayXeOZ2Nq_sbeDgtzMJpLLnXFgAChGV0aDKQtTA_KgEAAAAAIgEAAAAAAIJpZIJ2NIJpcISsaa0Zg2lwNpAkAIkHAAAAAPA8kv_-awoTiXNlY3AyNTZrMaEDHAD2JKYevx89W0CcFJFiskdcEzkH_Wdv9iW42qLK79ODdWRwgiMohHVkcDaCI4I"),
            ("lighthouse-london",  "enr:-Le4QLHZDSvkLfqgEo8IWGG96h6mxwe_PsggC20CL3neLBjfXLGAQFOPSltZ7oP6ol54OvaNqO02Rnvb8YmDR274uq8ChGV0aDKQtTA_KgEAAAAAIgEAAAAAAIJpZIJ2NIJpcISLosQxg2lwNpAqAX4AAAAAAPA8kv_-ax65iXNlY3AyNTZrMaEDBJj7_dLFACaxBfaI8KZTh_SSJUjhyAyfshimvSqo22WDdWRwgiMohHVkcDaCI4I"),
            ("teku-ohio",          "enr:-Iu4QLm7bZGdAt9NSeJG0cEnJohWcQTQaI9wFLu3Q7eHIDfrI4cwtzvEW3F3VbG9XdFXlrHyFGeXPn9snTCQJ9bnMRABgmlkgnY0gmlwhAOTJQCJc2VjcDI1NmsxoQIZdZD6tDYpkpEfVo5bgiU8MGRjhcOmHGD2nErK0UKRrIN0Y3CCIyiDdWRwgiMo"),
            ("nimbus-frankfurt",   "enr:-LK4QA8FfhaAjlb_BXsXxSfiysR7R52Nhi9JBt4F8SPssu8hdE1BXQQEtVDC3qStCW60LSO7hEsVHv5zm8_6Vnjhcn0Bh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhAN4aBKJc2VjcDI1NmsxoQJerDhsJ-KxZ8sHySMOCmTO6sHM3iCFQ6VMvLTe948MyYN0Y3CCI4yDdWRwgiOM"),
            // --- candidates: Prysm team (KG4Q) ---
            ("prysm-team-1",       "enr:-KG4QNTx85fjxABbSq_Rta9wy56nQ1fHK0PewJbGjLm1M4bMGx5-3Qq4ZX2-iFJ0pys_O90sVXNNOxp2E7afBsGsBrgDhGV0aDKQu6TalgMAAAD__________4JpZIJ2NIJpcIQEnfA2iXNlY3AyNTZrMaECGXWQ-rQ2KZKRH1aOW4IlPDBkY4XDphxg9pxKytFCkayDdGNwgiMog3VkcIIjKA"),
            ("prysm-team-2",       "enr:-KG4QF4B5WrlFcRhUU6dZETwY5ZzAXnA0vGC__L1Kdw602nDZwXSTs5RFXFIFUnbQJmhNGVU6OIX7KVrCSTODsz1tK4DhGV0aDKQu6TalgMAAAD__________4JpZIJ2NIJpcIQExNYEiXNlY3AyNTZrMaECQmM9vp7KhaXhI-nqL_R0ovULLCFSFTa9CPPSdb1zPX6DdGNwgiMog3VkcIIjKA"),
            // --- candidates: Pryslab bootstrap (Ku4Q) ---
            ("pryslab-boot-1",     "enr:-Ku4QImhMc1z8yCiNJ1TyUxdcfNucje3BGwEHzodEZUan8PherEo4sF7pPHPSIB1NNuSg5fZy7qFsjmUKs2ea1Whi0EBh2F0dG5ldHOIAAAAAAAAAACEZXRoMpD1pf1CAAAAAP__________gmlkgnY0gmlwhBLf22SJc2VjcDI1NmsxoQOVphkDqal4QzPMksc5wnpuC3gvSC8AfbFOnZY_On34wIN1ZHCCIyg"),
            ("pryslab-boot-2",     "enr:-Ku4QP2xDnEtUXIjzJ_DhlCRN9SN99RYQPJL92TMlSv7U5C1YnYLjwOQHgZIUXw6c-BvRg2Yc2QsZxxoS_pPRVe0yK8Bh2F0dG5ldHOIAAAAAAAAAACEZXRoMpD1pf1CAAAAAP__________gmlkgnY0gmlwhBLf22SJc2VjcDI1NmsxoQMeFF5GrS7UZpAH2Ly84aLK-TyvH-dRo0JM1i8yygH50YN1ZHCCJxA"),
            ("pryslab-boot-3",     "enr:-Ku4QPp9z1W4tAO8Ber_NQierYaOStqhDqQdOPY3bB3jDgkjcbk6YrEnVYIiCBbTxuar3CzS528d2iE7TdJsrL-dEKoBh2F0dG5ldHOIAAAAAAAAAACEZXRoMpD1pf1CAAAAAP__________gmlkgnY0gmlwhBLf22SJc2VjcDI1NmsxoQMw5fqqkw2hHC4F5HZZDPsNmPdB1Gi8JPQK7pRc9XHh-oN1ZHCCKvg"),
            // --- candidates: EF consensus (Ku4Q) ---
            ("ef-consensus-1",     "enr:-Ku4QHqVeJ8PPICcWk1vSn_XcSkjOkNiTg6Fmii5j6vUQgvzMc9L1goFnLKgXqBJspJjIsB91LTOleFmyWWrFVATGngBh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhAMRHkWJc2VjcDI1NmsxoQKLVXFOhp2uX6jeT0DvvDpPcU8FWMjQdR4wMuORMhpX24N1ZHCCIyg"),
            ("ef-consensus-2",     "enr:-Ku4QG-2_Md3sZIAUebGYT6g0SMskIml77l6yR-M_JXc-UdNHCmHQeOiMLbylPejyJsdAPsTHJyjJB2sYGDLe0dn8uYBh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhBLY-NyJc2VjcDI1NmsxoQORcM6e19T1T9gi7jxEZjk_sjVLGFscUNqAY9obgZaxbIN1ZHCCIyg"),
            ("ef-consensus-3",     "enr:-Ku4QPn5eVhcoF1opaFEvg1b6JNFD2rqVkHQ8HApOKK61OIcIXD127bKWgAtbwI7pnxx6cDyk_nI88TrZKQaGMZj0q0Bh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhDayLMaJc2VjcDI1NmsxoQK2sBOLGcUb4AwuYzFuAVCaNHA-dy24UuEKkeFNgCVCsIN1ZHCCIyg"),
            ("ef-consensus-4",     "enr:-Ku4QEWzdnVtXc2Q0ZVigfCGggOVB2Vc1ZCPEc6j21NIFLODSJbvNaef1g4PxhPwl_3kax86YPheFUSLXPRs98vvYsoBh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhDZBrP2Jc2VjcDI1NmsxoQM6jr8Rb1ktLEsVcKAPa08wCsKUmvoQ8khiOl_SLozf9IN1ZHCCIyg"),
            // --- candidates: Nimbus 2nd ---
            ("nimbus-2",           "enr:-LK4QKWrXTpV9T78hNG6s8AM6IO4XH9kFT91uZtFg1GcsJ6dKovDOr1jtAAFPnS2lvNltkOGA9k29BUN7lFh_sjuc9QBh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhANAdd-Jc2VjcDI1NmsxoQLQa6ai7y9PMN5hpLe5HmiJSlYzMuzP7ZhwRiwHvqNXdoN0Y3CCI4yDdWRwgiOM"),
        ];

        for (name, enr_str) in &enrs {
            let result = enr_str.parse::<discv5::enr::Enr<discv5::enr::CombinedKey>>();
            match result {
                Err(e) => println!("{name}: PARSE FAILED — {e}"),
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
