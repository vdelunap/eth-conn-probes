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
    #[serde(alias = "location_label", default)]
    pub network_label: Option<String>,
    pub app_channel: String,
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

    // Execution layer P2P
    #[serde(default)]
    pub p2p_boot_nodes: Vec<TcpTarget>,
    #[serde(default)]
    pub discv4_execution: Vec<TcpTarget>,
    /// Empty by default: execution boot nodes publish enode://, not enr:-.
    #[serde(default)]
    pub discv5_execution: Vec<Discv5Target>,
    #[serde(default)]
    pub rlpx_targets: Vec<RlpxTarget>,
    #[serde(default)]
    pub dns_compare: Vec<DnsCompareTarget>,

    // Consensus layer (Beacon chain)
    #[serde(default)]
    pub discv5_consensus: Vec<Discv5Target>,
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

/// enode://PUBKEY_HEX@IP:PORT. The pubkey is what ECIES needs, so RLPx targets
/// can't reuse TcpTarget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlpxTarget {
    pub name: String,
    pub enode: String,
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

/// Config baked into the desktop app (which has no TOML file to read).
pub fn default_config() -> Config {
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
    let rlpx = |name: &str, enode: &str| RlpxTarget {
        name: name.to_string(),
        enode: enode.to_string(),
    };

    Config {
        run: RunConfig {
            attempts: 3,
            min_successes: 1,
            timeout_ms: 5000,
            parallelism: 20,
        },
        client: ClientConfig {
            network_label: None,
            app_channel: "desktop".to_string(),
            client_id: String::new(),
        },
        reporting: ReportingConfig {
            enabled: true,
            report_url: "http://46.101.227.140:8000/report".to_string(),
            timeout_ms: 5000,
        },
        probes: ProbesConfig {
            control_http: ControlHttpProbe {
                enabled: false,
                url: "http://46.101.227.140:8000/ping".to_string(),
                expect_body: Some("ok".to_string()),
            },

            // Section 1: RPC provider availability.
            // Each entry expands into dns_resolve + tcp_connect + tls_handshake.
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

            // Expands into https_json_rpc + https_json_rpc_write.
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

            // Expands into wss_json_rpc + wss_subscribe.
            wss_jsonrpc: vec![
                wss("publicnode", "wss://ethereum-rpc.publicnode.com"),
                wss("drpc",       "wss://eth.drpc.org"),
            ],

            // Section 2: execution layer P2P.
            // IPs and pubkeys from go-ethereum params/bootnodes.go.
            // EF-hetzner-hel keeps the pubkey of the old EF-southeast-asia node (52.187.207.27),
            // which was relocated.
            p2p_boot_nodes: vec![
                boot("EF-ap-southeast",  "18.138.108.67",  30303),
                boot("EF-us-east",       "3.209.45.79",    30303),
                boot("EF-hetzner-hel",   "65.108.70.101",  30303),
                boot("EF-hetzner-fsn",   "157.90.35.166",  30303),
            ],

            discv4_execution: vec![
                boot("EF-ap-southeast",  "18.138.108.67",  30303),
                boot("EF-us-east",       "3.209.45.79",    30303),
                boot("EF-hetzner-hel",   "65.108.70.101",  30303),
                boot("EF-hetzner-fsn",   "157.90.35.166",  30303),
            ],

            // Execution boot nodes speak devp2p v4 (enode://), not discv5.
            discv5_execution: vec![],

            rlpx_targets: vec![
                rlpx("EF-ap-southeast",
                    "enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303"),
                rlpx("EF-us-east",
                    "enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303"),
                rlpx("EF-hetzner-hel",
                    "enode://2b252ab6a1d0f971d9722cb839a42cb81db019ba44c08754628ab4a823487071b5695317c8ccd085219c3a03af063495b2f1da8d18218da2d6a82981b45e6ffc@65.108.70.101:30303"),
                rlpx("EF-hetzner-fsn",
                    "enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303"),
            ],

            dns_compare: vec![
                dns_cmp("publicnode",         "ethereum-rpc.publicnode.com"),
                dns_cmp("cloudflare",         "cloudflare-eth.com"),
                dns_cmp("llamarpc",           "eth.llamarpc.com"),
                dns_cmp("drpc",               "eth.drpc.org"),
                dns_cmp("beacon-publicnode",  "ethereum-beacon-api.publicnode.com"),
                dns_cmp("beacon-chainsafe",   "lodestar-mainnet.chainsafe.io"),
            ],

            // Section 3: consensus layer (Beacon chain).
            // These ENRs only produce DiscV5 pings. The beacon TCP and libp2p targets come
            // from live Beacon API peers instead; see beacon_peers::fetch_all.
            discv5_consensus: vec![
                // Teku advertises tcp4=9000 but firewalls it: discovery only.
                Discv5Target { name: "teku-aws-ohio".into(),
                    enr: "enr:-Iu4QLm7bZGdAt9NSeJG0cEnJohWcQTQaI9wFLu3Q7eHIDfrI4cwtzvEW3F3VbG9XdFXlrHyFGeXPn9snTCQJ9bnMRABgmlkgnY0gmlwhAOTJQCJc2VjcDI1NmsxoQIZdZD6tDYpkpEfVo5bgiU8MGRjhcOmHGD2nErK0UKRrIN0Y3CCIyiDdWRwgiMo".into() },
                Discv5Target { name: "teku-aws-sydney".into(),
                    enr: "enr:-Iu4QEDJ4Wa_UQNbK8Ay1hFEkXvd8psolVK6OhfTL9irqz3nbXxxWyKwEplPfkju4zduVQj6mMhUCm9R2Lc4YM5jPcIBgmlkgnY0gmlwhANrfESJc2VjcDI1NmsxoQJCYz2-nsqFpeEj6eov9HSi9QssIVIVNr0I89J1vXM9foN0Y3CCIyiDdWRwgiMo".into() },
                // Nimbus advertises tcp4=9100, which is Prometheus, not libp2p. DiscV5 works.
                Discv5Target { name: "nimbus-frankfurt".into(),
                    enr: "enr:-LK4QA8FfhaAjlb_BXsXxSfiysR7R52Nhi9JBt4F8SPssu8hdE1BXQQEtVDC3qStCW60LSO7hEsVHv5zm8_6Vnjhcn0Bh2F0dG5ldHOIAAAAAAAAAACEZXRoMpC1MD8qAAAAAP__________gmlkgnY0gmlwhAN4aBKJc2VjcDI1NmsxoQJerDhsJ-KxZ8sHySMOCmTO6sHM3iCFQ6VMvLTe948MyYN0Y3CCI4yDdWRwgiOM".into() },
            ],

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
