export const defaultConfigToml = `[run]
attempts = 3
min_successes = 1
timeout_ms = 3500
parallelism = 16

[client]
location_label = "mobile"
app_channel = "app"

[reporting]
enabled = true
report_url = "http://146.146.146.146:8080/report"
timeout_ms = 3000

[probes.control_http]
enabled = true
url = "http://146.146.146.146:8080/ping"
expect_body = "ok"

[[probes.tcp]]
name = "ethereum-publicnode-https"
host = "ethereum-rpc.publicnode.com"
port = 443

[[probes.https_jsonrpc]]
name = "ethereum-publicnode-https"
url = "https://ethereum-rpc.publicnode.com"
method = "eth_chainId"
`