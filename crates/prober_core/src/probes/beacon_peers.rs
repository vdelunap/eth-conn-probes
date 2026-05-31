// Pulls the connected-peer list off the Beacon REST APIs
// (/eth/v1/node/peers?state=connected) to get real probe targets.
//
// Only direction="outbound" peers are kept: the beacon node dialled them, so we know
// they have a routable IP and an open listening port. Inbound peers may well be home
// validators behind NAT that nobody else can dial.
use crate::config;

pub struct LivePeer {
    pub host: String,
    pub port: u16,
    pub label: String,
}

/// Pulls (host, port) out of /ip4/A.B.C.D/tcp/PORT or /ip6/ADDR/tcp/PORT.
fn parse_multiaddr(addr: &str) -> Option<(String, u16)> {
    let parts: Vec<&str> = addr.split('/').collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "ip4" if i + 3 < parts.len() && parts[i + 2] == "tcp" => {
                if let Ok(port) = parts[i + 3].parse::<u16>() {
                    return Some((parts[i + 1].to_string(), port));
                }
            }
            "ip6" if i + 3 < parts.len() && parts[i + 2] == "tcp" => {
                let ip = parts[i + 1];
                if let Ok(port) = parts[i + 3].parse::<u16>() {
                    // Brackets, or TcpStream::connect won't parse it.
                    return Some((format!("[{ip}]"), port));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// RFC-1918, loopback, link-local, IPv6 ULA and CGNAT (RFC-6598).
fn is_non_routable(host: &str) -> bool {
    let h = host.trim_matches(|c| c == '[' || c == ']');
    let second_octet = || h.split('.').nth(1).and_then(|n| n.parse::<u8>().ok());
    h.starts_with("127.")
        || h.starts_with("10.")
        || h.starts_with("192.168.")
        || (h.starts_with("172.") && second_octet().is_some_and(|n| (16..=31).contains(&n)))
        || (h.starts_with("100.") && second_octet().is_some_and(|n| (64..=127).contains(&n)))
        || h == "::1"
        || h.starts_with("fe80:")
        || h.starts_with("fc00:")
        || h.starts_with("fd")
}

async fn fetch_from(url: &str, source_name: &str, timeout_ms: u64) -> Vec<(String, u16, String)> {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(source = source_name, error = %e, "beacon_peers: client build failed");
            return vec![];
        }
    };

    let endpoint = format!("{url}/eth/v1/node/peers?state=connected");
    let body: serde_json::Value = match client.get(&endpoint).send().await {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(source = source_name, error = %e, "beacon_peers: json parse failed");
                return vec![];
            }
        },
        Err(e) => {
            tracing::warn!(source = source_name, error = %e, "beacon_peers: request failed");
            return vec![];
        }
    };

    let data = match body.get("data").and_then(|d| d.as_array()) {
        Some(d) => d,
        None => {
            tracing::warn!(
                source = source_name,
                "beacon_peers: unexpected response shape"
            );
            return vec![];
        }
    };

    let mut out = Vec::new();
    for peer in data {
        let direction = peer
            .get("direction")
            .and_then(|d| d.as_str())
            .unwrap_or("inbound");
        if direction != "outbound" {
            continue;
        }

        let addr = peer
            .get("last_seen_p2p_address")
            .and_then(|a| a.as_str())
            .unwrap_or("");

        if let Some((host, port)) = parse_multiaddr(addr) {
            if !is_non_routable(&host) {
                out.push((
                    host.clone(),
                    port,
                    format!("{host}:{port} ({source_name} peer)"),
                ));
            }
        }
    }
    tracing::debug!(
        source = source_name,
        count = out.len(),
        "beacon_peers: fetched outbound"
    );
    out
}

/// Queries every configured beacon_https endpoint in parallel, deduplicating peers
/// by (host, port).
pub async fn fetch_all(cfg: &config::Config, timeout_ms: u64) -> Vec<LivePeer> {
    let handles: Vec<_> = cfg
        .probes
        .beacon_https
        .iter()
        .map(|t| {
            let url = t.url.clone();
            let name = t.name.clone();
            tokio::spawn(async move { fetch_from(&url, &name, timeout_ms).await })
        })
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut peers = Vec::new();

    for handle in handles {
        let batch = match handle.await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "beacon_peers: task panicked");
                continue;
            }
        };
        for (host, port, label) in batch {
            if seen.insert((host.clone(), port)) {
                peers.push(LivePeer { host, port, label });
            }
        }
    }

    tracing::info!(
        total = peers.len(),
        "beacon_peers: unique peers after dedup"
    );
    peers
}
