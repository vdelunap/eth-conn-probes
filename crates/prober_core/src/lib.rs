use anyhow::Context;

pub mod config;
pub mod model;
pub mod probes;
pub mod reporting;
pub mod rlp;

pub async fn run_plan(cfg: config::Config) -> anyhow::Result<model::Report> {
    // When both ring and aws-lc-rs are compiled in (reqwest pulls aws-lc-rs),
    // rustls::ClientConfig::builder() panics without an explicit default provider.
    // install_default() is a no-op if already set, so it's safe to call on every run.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let started_at_ms = model::now_ms();
    let run_id = uuid::Uuid::new_v4().to_string();

    let client = model::ClientInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        client_id: cfg.client.client_id.clone(),
        app_channel: cfg.client.app_channel.clone(),
        network_label: cfg.client.network_label.clone(),
    };

    let mut jobs = probes::build_jobs(&cfg).context("build_jobs")?;

    // Fetch live connected peers from Beacon API endpoints and add TCP + libp2p probes.
    // Remove the next two lines (and beacon_peers module) to disable this feature.
    let live_peers = probes::beacon_peers::fetch_all(&cfg, cfg.run.timeout_ms).await;
    jobs.extend(probes::build_live_peer_jobs(live_peers));

    let results = probes::run_jobs(&cfg.run, jobs).await;

    let finished_at_ms = model::now_ms();

    Ok(model::Report {
        run_id,
        timestamp: model::ms_to_iso8601(started_at_ms),
        started_at_ms,
        finished_at_ms,
        client,
        run: cfg.run.clone(),
        results,
    })
}
