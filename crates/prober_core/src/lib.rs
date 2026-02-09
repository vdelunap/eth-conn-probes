use anyhow::Context;

pub mod config;
pub mod model;
pub mod probes;
pub mod reporting;

pub async fn run_plan(cfg: config::Config) -> anyhow::Result<model::Report> {
    let started_at_ms = model::now_ms();
    let run_id = uuid::Uuid::new_v4().to_string();

    let client = model::ClientInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        location_label: cfg.client.location_label.clone(),
        app_channel: cfg.client.app_channel.clone(),
    };

    let jobs = probes::build_jobs(&cfg).context("build_jobs")?;
    let results = probes::run_jobs(&cfg.run, jobs).await;

    let finished_at_ms = model::now_ms();

    Ok(model::Report {
        run_id,
        started_at_ms,
        finished_at_ms,
        client,
        run: cfg.run.clone(),
        results,
    })
}
