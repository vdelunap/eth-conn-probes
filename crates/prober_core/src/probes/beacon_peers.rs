/// Fetches live inbound-connected peers from Beacon REST API nodes
/// (/eth/v1/node/peers?state=connected) and generates TCP + libp2p probe targets.
///
/// Only peers with direction="outbound" are used. "Outbound" from the beacon
/// node's perspective means the beacon node (publicnode/chainsafe) *initiated*
/// the connection TO that peer — proving the peer has a publicly routable IP
/// and an open listening port that the beacon node could dial successfully.
/// "Inbound" peers (nodes that connected TO the beacon node) are excluded:
/// they might be home validators behind NAT who can make outbound connections
/// but cannot accept unsolicited inbound connections from a third-party prober.
///
/// This filter is geographically neutral: if a well-connected infrastructure node
/// can reach a peer, any other infrastructure node on the public internet should
/// also be able to reach it, regardless of geographic location.
///
/// To remove this feature entirely:
///   - delete this file
///   - remove `pub mod beacon_peers;` from mod.rs
///   - remove `build_live_peer_jobs` from mod.rs
///   - remove the two beacon_peers lines from lib.rs::run_plan
use crate::config;

pub struct LivePeer {
    pub host: String,
    pub port: u16,
    pub label: String,
}

/// Parse a libp2p multiaddr string into (host, port).
/// Handles /ip4/A.B.C.D/tcp/PORT and /ip6/ADDR/tcp/PORT forms.
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
                    // Bracket IPv6 so TcpStream::connect parses it correctly.
                    return Some((format!("[{ip}]"), port));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Returns true for addresses that are not globally routable.
/// Includes RFC-1918 private ranges, loopback, link-local, IPv6 ULA,
/// and RFC-6598 Shared Address Space (100.64.0.0/10, CGNAT).
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
        // Only outbound peers: the beacon node (publicnode/chainsafe) initiated the
        // connection TO this peer. This proves the peer has a publicly routable IP
        // and an open listening port — the beacon node already reached it successfully.
        // Inbound peers (nodes that connected TO the beacon node) are excluded: they
        // may be behind NAT or residential firewalls. Being able to make an outbound
        // connection does not imply being reachable for unsolicited inbound connections.
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
        "beacon_peers: fetched inbound"
    );
    out
}

/// Fetch live peers from all configured beacon_https endpoints concurrently,
/// deduplicating by (host, port) across sources.
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
