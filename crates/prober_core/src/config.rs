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
    /// DiscV5 pings to execution layer boot nodes (ENR format, port 30303).
    /// Execution boot nodes advertise themselves via enode:// (devp2p v4), not enr:-.
    /// This list is therefore intentionally empty unless you have ENRs for execution nodes.
    #[serde(default)]
    pub discv5_execution: Vec<Discv5Target>,
    /// TCP connect probes to Ethereum execution boot nodes on port 30303.
    /// These test whether the RLPx/DiscV5 protocol port is reachable.
    #[serde(default)]
    pub p2p_boot_nodes: Vec<TcpTarget>,
    /// DiscV4 UDP ping probes to Ethereum execution boot nodes on port 30303.
    /// Tests whether the execution-layer peer discovery protocol (devp2p discv4) is reachable.
    #[serde(default)]
    pub discv4_execution: Vec<TcpTarget>,
    /// DNS comparison probes: resolves each host with both the system resolver
    /// and Cloudflare DoH (1.1.1.1) to detect DNS blocking or poisoning.
    #[serde(default)]
    pub dns_compare: Vec<DnsCompareTarget>,

    // --- Consensus layer (Beacon chain) ---

    /// DiscV5 pings to consensus boot nodes (ENR format, port 9000).
    /// Consensus nodes use enr:- format, making DiscV5 v5 pings viable here.
    #[serde(default)]
    pub discv5_consensus: Vec<Discv5Target>,
    /// HTTP GET probes to public beacon chain REST API endpoints.
    /// Hits /eth/v1/node/version to verify consensus data is accessible over HTTPS.
    #[serde(default)]
    pub beacon_https: Vec<BeaconHttpsTarget>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsCompareTarget {
    pub name: String,
    pub host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconHttpsTarget {
    pub name: String,
    pub url: String,
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
    let boot = |name: &str, ip: &str, port: u16| TcpTarget {
        name: name.to_string(),
        host: ip.to_string(),
        port,
    };
    let dns_cmp = |name: &str, host: &str| DnsCompareTarget {
        name: name.to_string(),
        host: host.to_string(),
    };

    Config {
        run: RunConfig {
            attempts: 3,
            min_successes: 1,
            timeout_ms: 5000,
            parallelism: 20,
        },
        client: ClientConfig {
            location_label: String::new(),
            app_channel: "desktop".to_string(),
            client_id: String::new(),
        },
        reporting: ReportingConfig {
            enabled: true,
            report_url: "http://146.146.146.146:8080/report".to_string(),
            timeout_ms: 5000,
        },
        probes: ProbesConfig {
            control_http: ControlHttpProbe {
                enabled: false,
                url: "http://146.146.146.146:8080/ping".to_string(),
                expect_body: Some("ok".to_string()),
            },

            // ---- Section 1: RPC provider availability ----
            // Each TCP entry also auto-generates a DNS resolve probe.

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

            // Only providers with confirmed public WebSocket endpoints.
            wss_jsonrpc: vec![
                wss("publicnode", "wss://ethereum-rpc.publicnode.com"),
                wss("drpc",       "wss://eth.drpc.org"),
            ],

            // Execution boot nodes use enode:// (devp2p v4), not enr:- (discv5 v5).
            // discv5_execution is empty because we have no usable ENRs for them.
            discv5_execution: vec![],

            // ---- Section 2: Ethereum execution layer P2P ----

            // Boot nodes are stable IPs maintained by the Ethereum Foundation.
            // Port 30303 is the standard RLPx + DiscV5 port.
            // If this port is blocked while port 443 works, it indicates
            // selective censorship of Ethereum's execution P2P layer.
            p2p_boot_nodes: vec![
                boot("EF-asia-pacific",   "18.138.108.67",  30303),
                boot("EF-us-east",        "3.209.45.79",    30303),
                boot("EF-southeast-asia", "52.187.207.27",  30303),
            ],

            // Same IPs as p2p_boot_nodes — tests UDP:30303 (discv4 discovery) separately from TCP.
            discv4_execution: vec![
                boot("EF-asia-pacific",   "18.138.108.67",  30303),
                boot("EF-us-east",        "3.209.45.79",    30303),
                boot("EF-southeast-asia", "52.187.207.27",  30303),
            ],

            // DNS comparison targets: the most widely used RPC provider domains.
            // Compares what the system DNS says vs. what Cloudflare 1.1.1.1 says.
            dns_compare: vec![
                dns_cmp("publicnode",         "ethereum-rpc.publicnode.com"),
                dns_cmp("cloudflare",         "cloudflare-eth.com"),
                dns_cmp("llamarpc",           "eth.llamarpc.com"),
                dns_cmp("drpc",               "eth.drpc.org"),
                dns_cmp("beacon-publicnode",  "ethereum-beacon-api.publicnode.com"),
                dns_cmp("beacon-chainsafe",   "lodestar-mainnet.chainsafe.io"),
            ],

            // ---- Section 3: Ethereum consensus layer (Beacon chain) ----

            // Consensus boot node ENRs sourced from Lighthouse, Teku, and EF.
            // These nodes advertise enr:- records, making DiscV5 v5 pings viable.
            discv5_consensus: vec![
                Discv5Target {
                    name: "teku-aws-ohio".to_string(),
                    enr: "enr:-Iu4QLm7bZGdAt9NSeJG0cEnJohWcQTQaI9wFLu3Q7eHIDfrI4cwtzvEW3F3VbG9XdFXlrHyFGeXPn9snTCQJ9bnMRABgmlkgnY0gmlwhAOTJQCJc2VjcDI1NmsxoQIZdZD6tDYpkpEfVo5bgiU8MGRjhcOmHGD2nErK0UKRrIN0Y3CCIyiDdWRwgiMo".to_string(),
                },
                Discv5Target {
                    name: "teku-aws-sydney".to_string(),
                    enr: "enr:-Iu4QEDJ4Wa_UQNbK8Ay1hFEkXvd8psolVK6OhfTL9irqz3nbXxxWyKwEplPfkju4zduVQj6mMhUCm9R2Lc4YM5jPcIBgmlkgnY0gmlwhANrfESJc2VjcDI1NmsxoQJCYz2-nsqFpeEj6eov9HSi9QssIVIVNr0I89J1vXM9foN0Y3CCIyiDdWRwgiMo".to_string(),
                },
                Discv5Target {
                    name: "lighthouse-sydney".to_string(),
                    enr: "enr:-Le4QPUXJS2BTORXxyx2Ia-9ae4YqA_JWX3ssj4E_J-3z1A-HmFGrU8BpvpqhNabayXeOZ2Nq_sbeDgtzMJpLLnXFgAChGV0aDKQtTA_KgEAAAAAIgEAAAAAAIJpZIJ2NIJpcISsaa0Zg2lwNpAkAIkHAAAAAPA8kv_-awoTiXNlY3AyNTZrMaEDHAD2JKYevx89W0CcFJFiskdcEzkH_Wdv9iW42qLK79ODdWRwgiMohHVkcDaCI4I".to_string(),
                },
                Discv5Target {
                    name: "lighthouse-london".to_string(),
                    enr: "enr:-Le4QLHZDSvkLfqgEo8IWGG96h6mxwe_PsggC20CL3neLBjfXLGAQFOPSltZ7oP6ol54OvaNqO02Rnvb8YmDR274uq8ChGV0aDKQtTA_KgEAAAAAIgEAAAAAAIJpZIJ2NIJpcISLosQxg2lwNpAqAX4AAAAAAPA8kv_-ax65iXNlY3AyNTZrMaEDBJj7_dLFACaxBfaI8KZTh_SSJUjhyAyfshimvSqo22WDdWRwgiMohHVkcDaCI4I".to_string(),
                },
                Discv5Target {
                    name: "nimbus-frankfurt".to_string(),
                    enr: "enr:-LK4QA8FfhaAjlb_BXsXxSfiysR7R52Nhi9JBt4F8SPssu8hdE1BXQQEtVDC3qStCW60LSO7hEsVHv5zm8_6Vnjhcn0Bh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhAN4aBKJc2VjcDI1NmsxoQJerDhsJ-KxZ8sHySMOCmTO6sHM3iCFQ6VMvLTe948MyYN0Y3CCI4yDdWRwgiOM".to_string(),
                },
            ],

            // Public beacon chain REST API endpoints (no API key required).
            // GET /eth/v1/node/version — tests HTTPS access to consensus-layer data.
            beacon_https: vec![
                BeaconHttpsTarget {
                    name: "publicnode".to_string(),
                    url: "https://ethereum-beacon-api.publicnode.com".to_string(),
                },
                BeaconHttpsTarget {
                    name: "chainsafe-lodestar".to_string(),
                    url: "https://lodestar-mainnet.chainsafe.io".to_string(),
                },
            ],
        },
    }
}
