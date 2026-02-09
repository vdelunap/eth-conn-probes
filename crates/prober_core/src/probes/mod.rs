use crate::{config, model};
use anyhow::Context;

pub mod dns;
pub mod http_control;
pub mod https_jsonrpc;
pub mod tcp;
pub mod wss_jsonrpc;

#[cfg(feature = "discv5")]
pub mod discv5_ping;

#[derive(Debug, Clone)]
pub struct ProbeJob {
    pub kind: model::ProbeKind,
    pub target_label: String,
    pub run: Box<dyn ProbeFn + Send + Sync>,
}

#[async_trait::async_trait]
pub trait ProbeFn {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult;
}

pub fn build_jobs(cfg: &config::Config) -> anyhow::Result<Vec<ProbeJob>> {
    let mut jobs: Vec<ProbeJob> = Vec::new();

    if cfg.probes.control_http.enabled {
        let url = cfg.probes.control_http.url.clone();
        let expect = cfg.probes.control_http.expect_body.clone();
        jobs.push(ProbeJob {
            kind: model::ProbeKind::HttpControl,
            target_label: url.clone(),
            run: Box::new(http_control::HttpControlProbe {
                url,
                expect_body: expect,
            }),
        });
    }

    for t in &cfg.probes.tcp {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::DnsResolve,
            target_label: format!("{} ({})", t.host, t.name),
            run: Box::new(dns::DnsResolveProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });

        jobs.push(ProbeJob {
            kind: model::ProbeKind::TcpConnect,
            target_label: format!("{}:{} ({})", t.host, t.port, t.name),
            run: Box::new(tcp::TcpConnectProbe {
                host: t.host.clone(),
                port: t.port,
            }),
        });
    }

    for t in &cfg.probes.https_jsonrpc {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::HttpsJsonRpc,
            target_label: format!("{} ({})", t.url, t.name),
            run: Box::new(https_jsonrpc::HttpsJsonRpcProbe {
                url: t.url.clone(),
                method: t.method.clone(),
            }),
        });
    }

    for t in &cfg.probes.wss_jsonrpc {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::WssJsonRpc,
            target_label: format!("{} ({})", t.url, t.name),
            run: Box::new(wss_jsonrpc::WssJsonRpcProbe {
                url: t.url.clone(),
                method: t.method.clone(),
            }),
        });
    }

    #[cfg(feature = "discv5")]
    for t in &cfg.probes.discv5_ping {
        jobs.push(ProbeJob {
            kind: model::ProbeKind::Discv5Ping,
            target_label: format!("{} ({})", &t.enr, &t.name),
            run: Box::new(discv5_ping::Discv5PingProbe { enr: t.enr.clone() }),
        });
    }

    Ok(jobs)
}

pub async fn run_jobs(run_cfg: &config::RunConfig, jobs: Vec<ProbeJob>) -> Vec<model::ProbeRun> {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(run_cfg.parallelism));
    let mut handles = Vec::new();

    for job in jobs {
        let sem = semaphore.clone();
        let attempts = run_cfg.attempts;
        let min_successes = run_cfg.min_successes;
        let timeout_ms = run_cfg.timeout_ms;

        handles.push(tokio::spawn(async move {
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
        }));
    }

    let mut out = Vec::new();
    for h in handles {
        if let Ok(r) = h.await.context("join probe task") {
            out.push(r);
        }
    }
    out
}
