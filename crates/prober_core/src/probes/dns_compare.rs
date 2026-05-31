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

        // Whatever resolver the OS is pointed at, i.e. the one that gets poisoned
        // or blocked where DNS censorship is in play.
        let addr_str = format!("{}:443", self.host);
        let system_result = tokio::time::timeout(timeout, lookup_host(&addr_str)).await;

        let system_ips: Vec<String> = match system_result {
            Ok(Ok(iter)) => iter.map(|sa| sa.ip().to_string()).collect(),
            _ => Vec::new(),
        };

        // The same query encrypted over HTTPS, skipping the local/ISP resolver entirely.
        let doh_ips = query_doh_cloudflare(&self.host, timeout_ms).await;

        let rtt_ms = model::now_ms().saturating_sub(started);

        let (ok, error, category) = match (system_ips.is_empty(), doh_ips.is_empty()) {
            // Neither worked: a plain DNS failure, not necessarily censorship.
            (true, true) => (
                false,
                Some("DNS resolution failed on both system and DoH".to_string()),
                "dns_failure",
            ),
            // DoH resolves what the local resolver won't: that's a block.
            (true, false) => (
                false,
                Some(format!(
                    "System DNS returned nothing (DoH resolved: {}); DNS block suspected",
                    doh_ips.join(", ")
                )),
                "dns_block",
            ),
            // System resolver worked, so nothing is being blocked here.
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
                // Disagreeing resolvers are usually just CDN anycast, occasionally
                // poisoning. Both sets go to the server so it can tell them apart.
                "ip_mismatch": !system_ips.is_empty()
                    && !doh_ips.is_empty()
                    && !system_ips.iter().any(|ip| doh_ips.contains(ip)),
            }),
        }
    }
}

/// A records for `host` from Cloudflare's DoH endpoint. Empty on any failure.
async fn query_doh_cloudflare(host: &str, timeout_ms: u64) -> Vec<String> {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

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

    // 0 is NOERROR; anything else means the name doesn't resolve.
    if json["Status"].as_u64() != Some(0) {
        return Vec::new();
    }

    // Type 1 answers are A records.
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
