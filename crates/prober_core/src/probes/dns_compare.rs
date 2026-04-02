use crate::model;
use tokio::net::lookup_host;

pub struct DnsCompareProbe {
    pub host: String,
}

#[async_trait::async_trait]
impl super::ProbeFn for DnsCompareProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();
        let timeout = std::time::Duration::from_millis(timeout_ms);

        // --- Step 1: Resolve with the system resolver ---
        // This uses whatever DNS server the OS is configured to use.
        // In countries with DNS-based censorship, this may be poisoned or blocked.
        let addr_str = format!("{}:443", self.host);
        let system_result = tokio::time::timeout(timeout, lookup_host(&addr_str)).await;

        let system_ips: Vec<String> = match system_result {
            Ok(Ok(iter)) => iter.map(|sa| sa.ip().to_string()).collect(),
            _ => Vec::new(),
        };

        // --- Step 2: Resolve with Cloudflare DoH (DNS-over-HTTPS at 1.1.1.1) ---
        // DoH sends the DNS query encrypted over HTTPS to 1.1.1.1.
        // It bypasses the local/ISP resolver entirely, so poisoning won't affect it.
        let doh_ips = query_doh_cloudflare(&self.host, timeout_ms).await;

        let rtt_ms = model::now_ms().saturating_sub(started);

        // --- Step 3: Compare and categorise ---
        let (ok, error, category) = match (system_ips.is_empty(), doh_ips.is_empty()) {
            // Both resolvers returned nothing — general DNS failure (not necessarily censorship).
            (true, true) => (
                false,
                Some("DNS resolution failed on both system and DoH".to_string()),
                "dns_failure",
            ),
            // System resolver returned nothing but DoH succeeded —
            // strong signal that the local/ISP resolver is blocking this domain.
            (true, false) => (
                false,
                Some(format!(
                    "System DNS returned nothing (DoH resolved: {}) — DNS block suspected",
                    doh_ips.join(", ")
                )),
                "dns_block",
            ),
            // System resolved OK (DoH also resolved, or DoH was unreachable).
            // Note: IP sets often differ legitimately due to CDN anycast routing —
            // a mismatch alone does NOT indicate poisoning without further analysis.
            // Both IP sets are stored in meta for the server to analyse.
            (false, _) => (true, None, "ok"),
        };

        model::AttemptResult {
            ok,
            rtt_ms: Some(rtt_ms),
            error,
            meta: serde_json::json!({
                "category": category,
                "system_ips": system_ips,
                "doh_ips": doh_ips,
                // Flag for server-side analysis: IPs differ between resolvers.
                // May indicate CDN (normal) or DNS poisoning (suspicious).
                "ip_mismatch": !system_ips.is_empty()
                    && !doh_ips.is_empty()
                    && !system_ips.iter().any(|ip| doh_ips.contains(ip)),
            }),
        }
    }
}

/// Queries Cloudflare's DNS-over-HTTPS endpoint for A records of the given host.
///
/// Returns a list of IPv4 addresses, or an empty list if the query fails or
/// the host doesn't resolve.
async fn query_doh_cloudflare(host: &str, timeout_ms: u64) -> Vec<String> {
    // Build a reqwest client. We set a short timeout so a blocked DoH endpoint
    // doesn't stall the whole probe.
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    // The DNS-over-HTTPS JSON API: returns A records for the requested hostname.
    let url = format!("https://1.1.1.1/dns-query?name={}&type=A", host);

    let resp = match client
        .get(&url)
        .header("Accept", "application/dns-json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    // Status 0 = NOERROR. Anything else means the domain doesn't resolve.
    if json["Status"].as_u64() != Some(0) {
        return Vec::new();
    }

    // Extract IP addresses from the Answer section (type 1 = A record).
    json["Answer"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|e| e["type"].as_u64() == Some(1))
                .filter_map(|e| e["data"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
