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
    /// Persistent device identifier. Generated once on first launch and stored locally.
    /// Set by the Tauri app before calling run_plan(); left empty for CLI use.
    #[serde(default)]
    pub client_id: String,
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

/// Returns the hardcoded default configuration used by the desktop app.
///
/// All public Ethereum RPC providers that don't require an API key are included.
/// For each provider we run: DNS resolve, TCP connect, HTTPS JSON-RPC.
/// Providers that support WebSocket also get a WSS JSON-RPC probe.
///
/// The reporting URL points to the self-hosted server on DigitalOcean.
/// TODO: replace 146.146.146.146 with the real server IP once deployed.
pub fn default_config() -> Config {
    // Helper closures to reduce repetition when building target lists.
    let tcp = |name: &str, host: &str| TcpTarget {
        name: name.to_string(),
        host: host.to_string(),
        port: 443,
    };
    let https = |name: &str, url: &str| JsonRpcTarget {
        name: name.to_string(),
        url: url.to_string(),
        method: "eth_chainId".to_string(),
    };
    let wss = |name: &str, url: &str| JsonRpcTarget {
        name: name.to_string(),
        url: url.to_string(),
        method: "eth_chainId".to_string(),
    };

    Config {
        run: RunConfig {
            attempts: 3,
            min_successes: 1,
            timeout_ms: 5000,
            parallelism: 16,
        },
        client: ClientConfig {
            // Location is derived from IP on the server side — not user input.
            location_label: String::new(),
            app_channel: "desktop".to_string(),
            // client_id is filled in by the Tauri app before calling run_plan().
            client_id: String::new(),
        },
        reporting: ReportingConfig {
            enabled: true,
            report_url: "http://146.146.146.146:8080/report".to_string(),
            timeout_ms: 5000,
        },
        probes: ProbesConfig {
            // Control probe checks that our own server is reachable.
            // Disabled until the server is deployed.
            control_http: ControlHttpProbe {
                enabled: false,
                url: "http://146.146.146.146:8080/ping".to_string(),
                expect_body: Some("ok".to_string()),
            },

            // TCP targets — from each entry, build_jobs() automatically creates
            // both a DNS resolve probe and a TCP connect probe.
            tcp: vec![
                tcp("publicnode",  "ethereum-rpc.publicnode.com"),
                tcp("cloudflare",  "cloudflare-eth.com"),
                tcp("llamarpc",    "eth.llamarpc.com"),
                tcp("drpc",        "eth.drpc.org"),
                tcp("flashbots",   "rpc.flashbots.net"),
                tcp("1rpc",        "1rpc.io"),
                tcp("mevblocker",  "rpc.mevblocker.io"),
                tcp("blast",       "eth-mainnet.public.blastapi.io"),
            ],

            // HTTPS JSON-RPC — sends eth_chainId and checks the response has a "result" field.
            https_jsonrpc: vec![
                https("publicnode", "https://ethereum-rpc.publicnode.com"),
                https("cloudflare", "https://cloudflare-eth.com"),
                https("llamarpc",   "https://eth.llamarpc.com"),
                https("drpc",       "https://eth.drpc.org"),
                https("flashbots",  "https://rpc.flashbots.net"),
                https("1rpc",       "https://1rpc.io/eth"),
                https("mevblocker", "https://rpc.mevblocker.io"),
                https("blast",      "https://eth-mainnet.public.blastapi.io"),
            ],

            // WSS JSON-RPC — only providers with confirmed public WebSocket endpoints.
            wss_jsonrpc: vec![
                wss("publicnode", "wss://ethereum-rpc.publicnode.com"),
                wss("drpc",       "wss://eth.drpc.org"),
                wss("blast",      "wss://eth-mainnet.public.blastapi.io"),
            ],

            // DiscV5 ping probes (disabled — requires the discv5 feature flag).
            discv5_ping: vec![],
        },
    }
}
