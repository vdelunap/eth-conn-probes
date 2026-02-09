use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub run: RunConfig,
    pub client: ClientConfig,
    pub reporting: ReportingConfig,
    pub probes: ProbesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub attempts: u32,
    pub min_successes: u32,
    pub timeout_ms: u64,
    pub parallelism: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub location_label: String,
    pub app_channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportingConfig {
    pub enabled: bool,
    pub report_url: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbesConfig {
    pub control_http: ControlHttpProbe,
    pub tcp: Vec<TcpTarget>,
    pub https_jsonrpc: Vec<JsonRpcTarget>,
    pub wss_jsonrpc: Vec<JsonRpcTarget>,
    pub discv5_ping: Vec<Discv5Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlHttpProbe {
    pub enabled: bool,
    pub url: String,
    pub expect_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpTarget {
    pub name: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcTarget {
    pub name: String,
    pub url: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discv5Target {
    pub name: String,
    pub enr: String,
}

impl Config {
    pub fn from_toml_str(input: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(input)?)
    }

    pub fn from_toml_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Self::from_toml_str(&s)
    }
}
