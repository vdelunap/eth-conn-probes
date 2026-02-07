# eth-conn-probes (Tauri + Rust + Server)

Cross-platform Ethereum connectivity prober (CLI + App) that validates reachability through:
- DNS resolve
- TCP connect (3-way handshake = “packets happened”)
- HTTPS JSON-RPC (e.g. eth_chainId) => DNS → TCP → TLS → HTTP → JSON-RPC
- WSS JSON-RPC (WebSocket handshake + JSON-RPC over WS)
- optional DiscV5 ping (UDP + discovery handshake)

It also ships a report collector server.

## Server
Reports are POSTed to:
- http://146.146.146.146:8080/report
Healthcheck:
- http://146.146.146.146:8080/ping -> ok

## CLI quickstart
```bash
cargo run -p prober_cli -- run --config config/default.toml --pretty