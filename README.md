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
| `tls_handshake` | TLS handshake. Extracts version, cipher suite, and certificate info |
| `https_json_rpc` | Full HTTPS + JSON-RPC `eth_chainId` (read path) |
| `https_json_rpc_write` | `eth_sendRawTransaction` with dummy payload. Detects write censorship (OFAC-style) |
| `wss_json_rpc` | WebSocket upgrade + JSON-RPC call over WSS |
| `wss_subscribe` | `eth_subscribe newHeads`. Confirms push subscriptions are available |
| `http_control` | Reachability of the collector itself. Off by default, useful when debugging it |

Each `[[probes.tcp]]` entry auto-generates `dns_resolve` + `tcp_connect` + `tls_handshake`.
Each `[[probes.https_jsonrpc]]` entry auto-generates `https_json_rpc` + `https_json_rpc_write`.
Each `[[probes.wss_jsonrpc]]` entry auto-generates `wss_json_rpc` + `wss_subscribe`.

Configured providers: `publicnode`, `cloudflare`, `llamarpc`, `drpc`, `flashbots`, `1rpc`, `mevblocker`, `blast`.

### Execution Layer P2P (port 30303)

| Probe | What it tests |
|---|---|
| `p2p_tcp_connect` | TCP connect to EF boot nodes on port 30303 (devp2p port) |
| `discv4_ping` | Full devp2p DiscV4 UDP handshake. Detects UDP-level blocking |
| `rlpx_handshake` | RLPx ECIES auth handshake. Detects DPI-based filtering of the RLPx transport |
| `dns_compare` | System resolver vs Cloudflare DoH (1.1.1.1). Detects DNS poisoning/blocking |
| `discv5_ping` | DiscV5 ping to an execution boot node. Unused by default (they speak devp2p v4) |

If port 443 works but 30303 fails → selective Ethereum P2P censorship.
If TCP works but UDP fails → DiscV4 peer discovery is blocked.
If TCP works but the RLPx auth packet times out → Deep Packet Inspection filtering RLPx traffic.

Boot nodes (from go-ethereum `params/bootnodes.go`): `EF-ap-southeast` (`18.138.108.67`),
`EF-us-east` (`3.209.45.79`), `EF-hetzner-hel` (`65.108.70.101`), `EF-hetzner-fsn` (`157.90.35.166`).

### Consensus Layer / Beacon Chain (ports 9000 and 443)

| Probe | What it tests |
|---|---|
| `beacon_discv5_ping` | DiscV5 ping to consensus boot nodes via ENR |
| `beacon_tcp_connect` | TCP connect to a live peer's libp2p port |
| `lib_p2p_handshake` | multistream-select `/noise` negotiation. Confirms a live libp2p node |
| `beacon_https` | HTTP GET `/eth/v1/node/version` to public Beacon REST APIs |

`beacon_tcp_connect` and `lib_p2p_handshake` targets are **not** taken from the configured ENRs, because boot nodes are discovery-only and firewall inbound TCP:9000, so probing them would only ever measure that firewall. Instead, `beacon_peers::fetch_all` queries `/eth/v1/node/peers?state=connected` on each `beacon_https` endpoint at the start of every run and keeps the `direction: "outbound"` peers: the beacon node dialled those itself, which proves they have a routable address and an open listening port. Non-routable addresses (RFC-1918, CGNAT, loopback, ULA) are dropped and the remainder deduplicated by `(host, port)`.

Consensus boot nodes: Teku (AWS Ohio, AWS Sydney), Nimbus (Frankfurt).
Beacon REST APIs: `publicnode`, `chainsafe-lodestar`.

---

## Architecture

```
eth-conn-probes/
├── crates/
│   ├── prober_core/          # Core library: probes, config, model, reporting
│   │   └── src/
│   │       ├── probes/       # One file per probe type
│   │       │   └── beacon_peers.rs  # Live peer discovery via Beacon REST API
│   │       ├── rlp.rs        # Minimal RLP encoder (no external deps)
│   │       ├── config.rs     # TOML config + hardcoded default
│   │       ├── model.rs      # Report / ProbeRun / AttemptResult types
│   │       └── reporting.rs  # POST /report + GET /api/geo-reports
│   └── prober_cli/           # CLI binary (eth-prober)
├── app/
│   ├── src/                  # React frontend
│   │   ├── App.jsx           # Probes tab: results tables per section
│   │   ├── MapView.jsx       # Map tab: MapLibre + OpenFreeMap tiles
│   │   └── lib/              # Tauri invoke wrappers, shared types
│   └── src-tauri/            # Tauri shell
│       └── src/
│           ├── commands.rs   # run_probes, flush_queued_reports, get_geo_reports
│           └── queue.rs      # Offline report queue (JSONL)
├── server/                   # FastAPI collector
│   ├── app/                  # main.py, storage.py, db.py, geoip.py
│   ├── base.sql              # Schema (ethconnprobes)
│   └── docker-compose.yml    # Postgres 16 + collector
├── docs/
│   └── DESIGN.md             # Technical design document
└── config/
    └── default.toml          # Default CLI configuration
```

The workspace has three Cargo members: `prober_core`, `prober_cli`, and `app/src-tauri`.

### prober_core

`run_plan(cfg) -> Report` is the single entry point. It:
1. Calls `build_jobs(&cfg)` to create a list of `ProbeJob` instances from the config.
2. Fetches live beacon peers and appends their TCP + libp2p jobs via `build_live_peer_jobs()`.
3. Runs all jobs concurrently through `run_jobs()` with a tokio semaphore limiting parallelism.
4. Each job runs `attempts` times; results are summarized into min/avg/max RTT and an `ok` bool.

A job that panics is reported as a failed `ProbeRun` with `category: "internal"` rather than being dropped.

Optional Cargo features:

```toml
[features]
discv4 = ["dep:k256", "dep:sha3", "dep:rand"]
discv5 = ["dep:discv5"]
rlpx   = ["dep:k256", "dep:sha3", "dep:rand", "dep:aes", "dep:ctr", "dep:cipher", "dep:hmac", "dep:sha2"]
```

The `discv4` probe implements the full devp2p handshake from scratch: RLP encoding, Keccak256 hashing, secp256k1 signing, and endpoint-proof reply.

The `rlpx` probe implements the EIP-8 auth packet: ECIES encryption (ephemeral ECDH + SHA-256 KDF + AES-128-CTR + HMAC-SHA256) using the remote's public key embedded in the enode:// URL.

The `discv5` feature gates the `beacon_discv5_ping` and `discv5_ping` probes only. `beacon_tcp_connect` and `lib_p2p_handshake` are always compiled; their targets come from the Beacon REST API at runtime, not from ENRs.

TLS, write-censorship, and WebSocket subscription probes are compiled unconditionally (no feature flag needed) because their dependencies (`rustls`, `reqwest`, and `tokio-tungstenite`) are already transitive dependencies of the base build.

### prober_cli

```
eth-prober run --config <path> [--no-send] [--pretty]
eth-prober print-default-config
```

- `run`: Loads config, runs all probes, prints JSON report to stdout, POSTs to `report_url` unless `--no-send` or `reporting.enabled = false`.
- `print-default-config`: Dumps the embedded default TOML.

### Tauri App

Uses the hardcoded `default_config()` from `prober_core`; it does not read `config/default.toml`. Two tabs: **Probes**, which runs a scan and shows one table per section, and **Map**, which plots every reported location on MapLibre with a filter by probe kind.

Three commands are exposed to the frontend:

- `run_probes(no_send: bool, network_label: Option<String>)`: runs all probes; if the server POST fails, the report is appended to `queued_reports.jsonl` in AppData for a later retry.
- `flush_queued_reports()`: drains the offline queue. Currently counts and clears entries without re-sending them; see the TODO in `queue.rs`.
- `get_geo_reports(kinds: Vec<String>)`: proxies `GET /api/geo-reports` for the map.

`network_label` is an optional self-reported tag (home, university, café, VPN…) that gives the collected data some context beyond the IP-derived location.

A persistent `client_id` UUID is generated on first launch and stored in AppData, allowing the server to correlate reports from the same device without collecting PII.

### Server

Docker Compose stack: Postgres 16 + a FastAPI collector (`server/app`, run under uvicorn).

```
POST /report            # Receives JSON reports from CLI and app clients (10/min per IP)
GET  /api/geo-reports   # GeoJSON FeatureCollection for the map (30/min per IP)
GET  /ping              # Health check → "ok"
```

`/report` rejects bodies over 512 KB, reports with more than 500 probe results, and anything without a valid UUID `run_id`; duplicate `run_id`s are ignored via `ON CONFLICT DO NOTHING`. Reports are stored across three tables (`reports`, `probe_runs`, `probe_attempts`) in the `ethconnprobes` schema; see `server/base.sql`.

GeoIP enrichment happens server-side from GeoLite2 mmdb files mounted read-only at `/geoip`; if they are absent, reports are stored without location and simply won't appear on the map.

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
    "app_channel": "cli",
    "network_label": "home"
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
network_label = ""     # Self-reported context (home, university, vpn…); optional
app_channel   = "cli"
client_id     = ""     # Set by the Tauri app; leave empty for CLI

[reporting]
enabled    = true
report_url = "http://<server>:8000/report"
timeout_ms = 5000

[probes.control_http]
enabled     = false
url         = "http://<server>:8000/ping"
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

[[probes.discv5_consensus]] # DiscV5 ping only (feature: discv5)
name = "teku-aws-ohio"
enr  = "enr:-Iu4Q..."

[[probes.beacon_https]]   # GET /eth/v1/node/version, and the source of live peers
name = "publicnode"
url  = "https://ethereum-beacon-api.publicnode.com"
```

`location_label` is accepted as an alias for `network_label` so older config files keep working.

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

The stack reads its credentials from Docker secrets, so they have to exist before the
first `up`:

```bash
cd server
mkdir -p secrets/postgres geoip
echo 'a-strong-password'          > secrets/postgres/pgfile   # POSTGRES_PASSWORD_FILE
printf '[main]\nhost=postgres\nport=5432\ndbname=dbalfa\nuser=ethprobes\npassword=…\n' \
                                  > secrets/postgres/pgs      # PGSERVICEFILE for the API
# base.sql creates the ethprobes/dbuser roles without passwords; put the matching
# ALTER USER … PASSWORD statements in secrets/postgres/auth.sql

# Optional: drop GeoLite2-City.mmdb and GeoLite2-ASN.mmdb into ./geoip for
# server-side location enrichment. Without them reports are stored unlocated.

docker compose up -d
```

The collector listens on host port `8000` and Postgres is published on `15432` for
inspection, so firewall it or drop the `ports` block if you don't need it.

---

## DNS Compare: censorship detection

The `dns_compare` probe resolves each hostname twice:
1. **System resolver**: whatever DNS the OS uses (may be poisoned by ISP).
2. **Cloudflare DoH**: encrypted DNS-over-HTTPS to `1.1.1.1`, bypasses the local resolver entirely.

| Result | Interpretation |
|---|---|
| System returns IPs | OK (DoH IPs also stored for server analysis) |
| System empty, DoH has IPs | DNS block suspected |
| Both empty | General DNS failure |
| IPs differ between resolvers | Flagged as `ip_mismatch`. May be CDN routing (normal) or DNS poisoning (suspicious), requires server-side analysis |

## Write Censorship Detection

The `https_json_rpc_write` probe sends `eth_sendRawTransaction` with the payload `"0x"` (an invalid transaction). A functioning provider returns a JSON-RPC error (e.g., `-32000 "invalid transaction"`). Both a `result` and an `error` field in the response count as `ok=true`; what matters is that the write endpoint processed the request.

A provider that returns HTTP 403, times out, or returns no response is flagged as `ok=false` with `category=auth_required` or `category=network`, signalling potential OFAC-compliance filtering of the write path while the read path still works.
