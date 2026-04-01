use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub run_id: String,
    /// ISO 8601 UTC timestamp of when the run started (e.g. "2026-04-02T14:30:00.123Z").
    /// Use this field for database storage.
    pub timestamp: String,
    /// Unix epoch in milliseconds — kept for precision and legacy compatibility.
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
    pub client: ClientInfo,
    pub run: crate::config::RunConfig,
    pub results: Vec<ProbeRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub os: String,
    pub arch: String,
    /// Persistent random UUID generated on first launch and stored locally.
    /// Identifies the device across multiple report submissions without collecting PII.
    pub client_id: String,
    pub app_channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRun {
    pub kind: ProbeKind,
    pub target: String,
    pub attempts: Vec<AttemptResult>,
    pub summary: ProbeSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeSummary {
    pub success_count: u32,
    pub failure_count: u32,
    pub min_rtt_ms: Option<u128>,
    pub avg_rtt_ms: Option<u128>,
    pub max_rtt_ms: Option<u128>,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptResult {
    pub ok: bool,
    pub rtt_ms: Option<u128>,
    pub error: Option<String>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeKind {
    DnsResolve,
    TcpConnect,
    HttpControl,
    HttpsJsonRpc,
    WssJsonRpc,
    Discv5Ping,
}

pub fn now_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Formats a Unix millisecond timestamp as an ISO 8601 UTC string.
pub fn ms_to_iso8601(ms: u128) -> String {
    use chrono::{DateTime, Utc};
    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    let dt = DateTime::<Utc>::from_timestamp(secs, nanos)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn summarize_attempts(attempts: &[AttemptResult], min_successes: u32) -> ProbeSummary {
    let mut ok_count: u32 = 0;
    let mut fail_count: u32 = 0;
    let mut rtts: Vec<u128> = Vec::new();

    for a in attempts {
        if a.ok {
            ok_count += 1;
        } else {
            fail_count += 1;
        }
        if let Some(rtt) = a.rtt_ms {
            rtts.push(rtt);
        }
    }

    rtts.sort_unstable();
    let min_rtt_ms = rtts.first().copied();
    let max_rtt_ms = rtts.last().copied();
    let avg_rtt_ms = if rtts.is_empty() {
        None
    } else {
        Some(rtts.iter().sum::<u128>() / (rtts.len() as u128))
    };

    ProbeSummary {
        success_count: ok_count,
        failure_count: fail_count,
        min_rtt_ms,
        avg_rtt_ms,
        max_rtt_ms,
        ok: ok_count >= min_successes,
    }
}
