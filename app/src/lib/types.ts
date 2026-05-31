export type ProbeKind =
  | 'dns_resolve'
  | 'tcp_connect'
  | 'tls_handshake'
  | 'http_control'
  | 'https_json_rpc'
  | 'https_json_rpc_write'
  | 'wss_json_rpc'
  | 'wss_subscribe'
  | 'p2p_tcp_connect'
  | 'dns_compare'
  | 'discv5_ping'
  | 'discv4_ping'
  | 'rlpx_handshake'
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
  client_id: string
  app_channel: string
  network_label?: string
}

export type Report = {
  run_id: string
  timestamp: string
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
  | { kind: 'skipped' }

export type RunResponse = {
  report: Report
  send: SendOutcome
}

// ok_count/fail_count are relative to whatever probe-kind filter was applied.
export type GeoFeatureProperties = {
  country_iso: string | null
  country_name: string | null
  city_name: string | null
  network_label: string | null
  received_at: string
  ok_count: number
  fail_count: number
  total: number
}

export type GeoFeature = {
  type: 'Feature'
  geometry: { type: 'Point'; coordinates: [number, number] }
  properties: GeoFeatureProperties
}

export type GeoFeatureCollection = {
  type: 'FeatureCollection'
  features: Array<GeoFeature>
}
