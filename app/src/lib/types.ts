export type ProbeKind =
  // RPC provider availability
  | 'dns_resolve'
  | 'tcp_connect'
  | 'tls_handshake'
  | 'http_control'
  | 'https_json_rpc'
  | 'https_json_rpc_write'
  | 'wss_json_rpc'
  | 'wss_subscribe'
  // Execution layer P2P
  | 'p2p_tcp_connect'
  | 'dns_compare'
  | 'discv5_ping'
  | 'discv4_ping'
  | 'rlpx_handshake'
  // Consensus layer (Beacon chain)
  | 'beacon_discv5_ping'
  | 'beacon_tcp_connect'
  | 'lib_p2p_handshake'
  | 'beacon_https'

export type AttemptResult = {
  ok: boolean
  rtt_ms: number | null
  error: string | null
  meta: unknown
}

export type ProbeSummary = {
  success_count: number
  failure_count: number
  min_rtt_ms: number | null
  avg_rtt_ms: number | null
  max_rtt_ms: number | null
  ok: boolean
}

export type ProbeRun = {
  kind: ProbeKind
  target: string
  attempts: Array<AttemptResult>
  summary: ProbeSummary
}

export type ClientInfo = {
  os: string
  arch: string
  location_label: string
  app_channel: string
}

export type Report = {
  run_id: string
  started_at_ms: number
  finished_at_ms: number
  client: ClientInfo
  run: {
    attempts: number
    min_successes: number
    timeout_ms: number
    parallelism: number
  }
  results: Array<ProbeRun>
}

export type SendOutcome =
  | { kind: 'sent' }
  | { kind: 'queued'; reason: string }

export type RunResponse = {
  report: Report
  send: SendOutcome
}
