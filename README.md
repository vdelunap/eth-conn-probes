# eth-conn-probes

A cross-platform Ethereum connectivity prober that detects network-level censorship and reachability issues across the full Ethereum stack: RPC providers, execution P2P layer, and consensus beacon chain.

Ships as a **CLI tool** and a **Tauri desktop app**, both backed by the same Rust core library. A self-hosted server collects reports from all clients.

---

## What it probes

Each run executes all probe types in parallel and produces a structured JSON report.

### RPC Provider Availability (port 443)

| Probe | What it tests |
|---|---|
| `dns_resolve` | System DNS resolves the provider hostname |
| `tcp_connect` | TCP 3-way handshake to port 443 |
| `tls_handshake` | TLS 1.3 handshake — extracts version, cipher suite, and certificate info |
| `https_json_rpc` | Full HTTPS + JSON-RPC `eth_chainId` (read path) |
| `https_json_rpc_write` | `eth_sendRawTransaction` with dummy payload — detects write censorship (OFAC-style) |
| `wss_json_rpc` | WebSocket upgrade + JSON-RPC call over WSS |
| `wss_subscribe` | `eth_subscribe newHeads` — confirms push subscriptions are available |
| `dns_compare` | System resolver vs Cloudflare DoH (1.1.1.1) — detects DNS poisoning/blocking |

Each `[[probes.tcp]]` entry auto-generates `dns_resolve` + `tcp_connect` + `tls_handshake`.
Each `[[probes.https_jsonrpc]]` entry auto-generates `https_json_rpc` + `https_json_rpc_write`.
Each `[[probes.wss_jsonrpc]]` entry auto-generates `wss_json_rpc` + `wss_subscribe`.

Configured providers: `publicnode`, `cloudflare`, `llamarpc`, `drpc`, `flashbots`, `1rpc`, `mevblocker`, `blast`.

### Execution Layer P2P (port 30303)

| Probe | What it tests |
|---|---|
| `p2p_tcp_connect` | TCP connect to EF boot nodes on port 30303 (devp2p port) |
| `discv4_ping` | Full devp2p DiscV4 UDP handshake — detects UDP-level blocking |
| `rlpx_handshake` | RLPx ECIES auth handshake — detects DPI-based filtering of the RLPx transport |

If port 443 works but 30303 fails → selective Ethereum P2P censorship.
If TCP works but UDP fails → DiscV4 peer discovery is blocked.
If TCP works but RLPx auth is rejected → Deep Packet Inspection filtering RLPx traffic.

Boot nodes: EF Asia-Pacific (`18.138.108.67`), EF US-East (`3.209.45.79`), EF Southeast Asia (`52.187.207.27`).

### Consensus Layer / Beacon Chain (port 9000)

| Probe | What it tests |
|---|---|
| `beacon_discv5_ping` | DiscV5 ping to consensus boot nodes via ENR |
| `beacon_tcp_connect` | TCP connect to port 9000 — derived from ENR `ip4`/`tcp4` fields |
| `libp2p_handshake` | multistream-select `/noise` negotiation — confirms a live libp2p node |
| `beacon_https` | HTTP GET `/eth/v1/node/version` to public Beacon REST APIs |

`beacon_tcp_connect` and `libp2p_handshake` targets are derived automatically from the same ENR entries used for `beacon_discv5_ping` — the `ip4` and `tcp4` fields are decoded to get the host and port.

Consensus nodes: Teku (AWS Ohio, Sydney), Lighthouse (Sydney, London), Nimbus (Frankfurt).
Beacon REST APIs: `publicnode`, `chainsafe-lodestar`.

---

## Architecture

```
eth-conn-probes/
├── crates/
│   ├── prober_core/        # Core library: probes, config, model, reporting
│   │   └── src/
│   │       ├── probes/     # One file per probe type
│   │       ├── rlp.rs      # Shared minimal RLP encoder (no external deps)
│   │       ├── config.rs   # TOML-deserialized config + hardcoded default
│   │       ├── model.rs    # Report / ProbeRun / AttemptResult types
│   │       └── reporting.rs# HTTP POST to the collector server
│   └── prober_cli/         # CLI binary (eth-prober)
├── app/
│   └── src-tauri/          # Tauri desktop app
│       └── src/
│           ├── commands.rs # Tauri commands: run_probes, flush_queued_reports
│           └── queue.rs    # Offline report queue (JSONL)
├── docs/
│   └── DESIGN.md           # Technical design document (TFM format)
├── server/
│   └── docker-compose.yml  # Postgres + eth-prober-server collector
└── config/
    └── default.toml        # Default CLI configuration
```

The workspace has three Cargo members: `prober_core`, `prober_cli`, and `app/src-tauri`.

### prober_core

`run_plan(cfg) -> Report` is the single entry point. It:
1. Calls `build_jobs(&cfg)` to create a list of `ProbeJob` instances from the config.
2. Runs all jobs concurrently through `run_jobs()` with a tokio semaphore limiting parallelism.
3. Each job runs `attempts` times; results are summarized into min/avg/max RTT and an `ok` bool.

Optional Cargo features:

```toml
[features]
discv4 = ["dep:k256", "dep:sha3", "dep:rand"]
discv5 = ["dep:discv5"]
rlpx   = ["dep:k256", "dep:sha3", "dep:rand", "dep:aes", "dep:ctr", "dep:cipher", "dep:hmac", "dep:sha2"]
```

The `discv4` probe implements the full devp2p handshake from scratch: RLP encoding, Keccak256 hashing, secp256k1 signing, and endpoint-proof reply.

The `rlpx` probe implements the EIP-8 auth packet: ECIES encryption (ephemeral ECDH + SHA-256 KDF + AES-128-CTR + HMAC-SHA256) using the remote's public key embedded in the enode:// URL.

The `discv5` feature also enables automatic derivation of `beacon_tcp_connect` and `libp2p_handshake` probes by decoding the `ip4`/`tcp4` fields from consensus ENRs.

TLS, write-censorship, and WebSocket subscription probes are compiled unconditionally (no feature flag needed) because their dependencies — `rustls`, `reqwest`, and `tokio-tungstenite` — are already transitive dependencies of the base build.

### prober_cli

```
eth-prober run --config <path> [--no-send] [--pretty]
eth-prober print-default-config
```

- `run`: Loads config, runs all probes, prints JSON report to stdout, POSTs to `report_url` unless `--no-send` or `reporting.enabled = false`.
- `print-default-config`: Dumps the embedded default TOML.

### Tauri App

Uses the hardcoded `default_config()` from `prober_core`. Exposes two commands to the frontend:

- `run_probes(no_send: bool)` — runs all probes; if the server POST fails, the report is appended to `queued_reports.jsonl` in AppData for a later retry.
- `flush_queued_reports()` — drains the offline queue.

A persistent `client_id` UUID is generated on first launch and stored in AppData, allowing the server to correlate reports from the same device without collecting PII.

### Server

Docker Compose stack: Postgres 16 + Rust HTTP server.

```
POST /report    # Receives JSON reports from CLI and app clients
GET  /ping      # Health check → "ok"
```

The DB is internal-only (no exposed port). GeoIP lookup is mounted at `/geoip` for server-side location enrichment.

---

## Report format

```json
{
  "run_id": "uuid-v4",
  "timestamp": "2026-05-13T10:00:00.000Z",
  "started_at_ms": 1747123200000,
  "finished_at_ms": 1747123205000,
  "client": {
    "os": "linux",
    "arch": "x86_64",
    "client_id": "uuid-v4",
    "app_channel": "cli"
  },
  "run": { "attempts": 3, "min_successes": 1, "timeout_ms": 5000, "parallelism": 16 },
  "results": [
    {
      "kind": "dns_resolve",
      "target": "ethereum-rpc.publicnode.com (publicnode)",
      "attempts": [
        { "ok": true, "rtt_ms": 42, "error": null, "meta": { "addrs": ["..."] } }
      ],
      "summary": { "success_count": 3, "failure_count": 0, "min_rtt_ms": 38, "avg_rtt_ms": 41, "max_rtt_ms": 45, "ok": true }
    }
  ]
}
```

`ok` in `summary` is `true` when `success_count >= min_successes`.

---

## Configuration

```toml
[run]
attempts      = 3      # Attempts per probe
min_successes = 1      # Successes needed for probe to be "ok"
timeout_ms    = 5000
parallelism   = 16     # Max concurrent probes

[client]
location_label = ""    # Informational; server derives location from IP
app_channel    = "cli"
client_id      = ""    # Set by Tauri app; leave empty for CLI

[reporting]
enabled    = true
report_url = "http://<server>:8080/report"
timeout_ms = 5000

[probes.control_http]
enabled     = false
url         = "http://<server>:8080/ping"
expect_body = "ok"

[[probes.tcp]]            # Auto-generates dns_resolve + tcp_connect + tls_handshake
name = "publicnode"
host = "ethereum-rpc.publicnode.com"
port = 443

[[probes.https_jsonrpc]]  # Auto-generates https_json_rpc + https_json_rpc_write
name   = "publicnode"
url    = "https://ethereum-rpc.publicnode.com"
method = "eth_chainId"

[[probes.wss_jsonrpc]]    # Auto-generates wss_json_rpc + wss_subscribe
name   = "publicnode"
url    = "wss://ethereum-rpc.publicnode.com"
method = "eth_chainId"

[[probes.p2p_boot_nodes]] # TCP to port 30303
name = "EF-us-east"
host = "3.209.45.79"
port = 30303

[[probes.discv4_execution]] # UDP DiscV4 ping to port 30303 (feature: discv4)
name = "EF-us-east"
host = "3.209.45.79"
port = 30303

[[probes.rlpx_targets]]   # RLPx ECIES auth handshake (feature: rlpx)
name  = "EF-us-east"
enode = "enode://22a8232c...@3.209.45.79:30303"

[[probes.dns_compare]]    # System resolver vs Cloudflare DoH
name = "publicnode"
host = "ethereum-rpc.publicnode.com"

[[probes.discv5_consensus]] # DiscV5 ping + beacon_tcp_connect + libp2p_handshake (feature: discv5)
name = "teku-aws-ohio"
enr  = "enr:-Iu4Q..."

[[probes.beacon_https]]   # GET /eth/v1/node/version
name = "publicnode"
url  = "https://ethereum-beacon-api.publicnode.com"
```

---

## Quickstart

### CLI

```bash
# Run with default config, pretty-print results, skip server upload
cargo run -p prober_cli -- run --config config/default.toml --pretty --no-send

# Run and send report to server
cargo run -p prober_cli -- run --config config/default.toml

# Print the embedded default config
cargo run -p prober_cli -- print-default-config

# Enable all optional P2P probes
cargo run -p prober_cli --features prober_core/discv4,prober_core/discv5,prober_core/rlpx \
  -- run --config config/default.toml --pretty --no-send
```

### Desktop App

```bash
cd app
npm install
npm run tauri dev
```

### Server

```bash
cd server
POSTGRES_PASSWORD=secret docker compose up -d
```

---

## DNS Compare — censorship detection

The `dns_compare` probe resolves each hostname twice:
1. **System resolver** — whatever DNS the OS uses (may be poisoned by ISP).
2. **Cloudflare DoH** — encrypted DNS-over-HTTPS to `1.1.1.1`, bypasses the local resolver entirely.

| Result | Interpretation |
|---|---|
| System returns IPs | OK (DoH IPs also stored for server analysis) |
| System empty, DoH has IPs | DNS block suspected |
| Both empty | General DNS failure |
| IPs differ between resolvers | Flagged as `ip_mismatch` — may be CDN routing (normal) or DNS poisoning (suspicious), requires server-side analysis |

## Write Censorship Detection

The `https_json_rpc_write` probe sends `eth_sendRawTransaction` with the payload `"0x"` (an invalid transaction). A functioning provider returns a JSON-RPC error (e.g., `-32000 "invalid transaction"`). Both a `result` and an `error` field in the response count as `ok=true` — what matters is that the write endpoint processed the request.

A provider that returns HTTP 403, times out, or returns no response is flagged as `ok=false` with `category=auth_required` or `category=network`, signalling potential OFAC-compliance filtering of the write path while the read path still works.
