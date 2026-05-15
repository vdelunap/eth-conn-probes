import { useState, useMemo } from 'react'
import { runProbes } from './lib/api.js'

// --- Styles ---

const S = {
  page: {
    minHeight: '100vh',
    background: '#0f1117',
    color: '#e2e8f0',
    fontFamily: 'system-ui, -apple-system, sans-serif',
    padding: '32px 24px',
    boxSizing: 'border-box',
  },
  inner: {
    maxWidth: 860,
    margin: '0 auto',
  },
  title: {
    margin: '0 0 4px',
    fontSize: 22,
    fontWeight: 700,
    color: '#f8fafc',
    letterSpacing: '-0.3px',
  },
  subtitle: {
    margin: '0 0 28px',
    fontSize: 13,
    color: '#94a3b8',
    lineHeight: 1.5,
  },
  controls: {
    display: 'flex',
    alignItems: 'center',
    gap: 16,
    marginBottom: 28,
  },
  toggle: {
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    fontSize: 13,
    color: '#cbd5e1',
    cursor: 'pointer',
    userSelect: 'none',
  },
  checkbox: {
    width: 15,
    height: 15,
    accentColor: '#6366f1',
    cursor: 'pointer',
  },
  button: {
    padding: '8px 20px',
    fontSize: 13,
    fontWeight: 600,
    background: '#6366f1',
    color: '#fff',
    border: 'none',
    borderRadius: 6,
    cursor: 'pointer',
    transition: 'background 0.15s',
  },
  buttonDisabled: {
    background: '#3730a3',
    color: '#a5b4fc',
    cursor: 'not-allowed',
  },
  spinner: {
    display: 'inline-block',
    width: 12,
    height: 12,
    border: '2px solid #a5b4fc',
    borderTopColor: 'transparent',
    borderRadius: '50%',
    animation: 'spin 0.7s linear infinite',
    marginRight: 7,
    verticalAlign: 'middle',
  },
  sendBadge: (kind) => ({
    display: 'inline-block',
    padding: '2px 9px',
    borderRadius: 4,
    fontSize: 11,
    fontWeight: 600,
    background: kind === 'sent' ? '#14532d' : kind === 'failed' ? '#450a0a' : '#1e293b',
    color: kind === 'sent' ? '#86efac' : kind === 'failed' ? '#fca5a5' : '#94a3b8',
    border: `1px solid ${kind === 'sent' ? '#166534' : kind === 'failed' ? '#7f1d1d' : '#334155'}`,
  }),
  errorBox: {
    padding: '10px 14px',
    background: '#450a0a',
    border: '1px solid #7f1d1d',
    borderRadius: 6,
    color: '#fca5a5',
    fontSize: 13,
    marginBottom: 20,
  },
  section: {
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 11,
    fontWeight: 700,
    color: '#64748b',
    letterSpacing: '0.08em',
    textTransform: 'uppercase',
    marginBottom: 10,
  },
  table: {
    width: '100%',
    borderCollapse: 'collapse',
    fontSize: 13,
  },
  th: {
    textAlign: 'left',
    padding: '7px 10px',
    fontSize: 11,
    fontWeight: 600,
    color: '#64748b',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
    borderBottom: '1px solid #1e293b',
  },
  thRight: {
    textAlign: 'right',
    padding: '7px 10px',
    fontSize: 11,
    fontWeight: 600,
    color: '#64748b',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
    borderBottom: '1px solid #1e293b',
  },
  td: {
    padding: '7px 10px',
    borderBottom: '1px solid #1e293b',
    color: '#cbd5e1',
  },
  tdRight: {
    padding: '7px 10px',
    borderBottom: '1px solid #1e293b',
    color: '#cbd5e1',
    textAlign: 'right',
    fontVariantNumeric: 'tabular-nums',
  },
  okBadge: (ok) => ({
    display: 'inline-block',
    padding: '1px 7px',
    borderRadius: 3,
    fontSize: 11,
    fontWeight: 700,
    background: ok ? '#14532d' : '#450a0a',
    color: ok ? '#86efac' : '#fca5a5',
  }),
  kindTag: {
    display: 'inline-block',
    padding: '1px 7px',
    borderRadius: 3,
    fontSize: 11,
    fontWeight: 600,
    background: '#1e293b',
    color: '#94a3b8',
    fontFamily: 'ui-monospace, monospace',
  },
  reasonText: (category) => {
    const colors = {
      rate_limited:  '#f59e0b',  // amber  — free-tier throttle, not a real block
      auth_required: '#a78bfa',  // purple — provider requires an API key
      rpc_error:     '#f97316',  // orange — provider replied but with an error
      api_error:     '#f97316',  // orange — unexpected response structure
      http_error:    '#ef4444',  // red    — bad HTTP status
      network:       '#ef4444',  // red    — TCP/connection failure
      dns_error:     '#ef4444',  // red    — name not resolved
      timeout:       '#ef4444',  // red    — no response in time
    }
    return {
      fontSize: 11,
      color: colors[category] ?? '#94a3b8',
      fontFamily: 'ui-monospace, monospace',
    }
  },
  statRow: {
    display: 'flex',
    gap: 24,
    marginBottom: 16,
    flexWrap: 'wrap',
  },
  stat: {
    fontSize: 13,
    color: '#94a3b8',
  },
  statNum: {
    fontWeight: 700,
    color: '#e2e8f0',
  },
  thSortable: {
    textAlign: 'left',
    padding: '7px 10px',
    fontSize: 11,
    fontWeight: 600,
    color: '#64748b',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
    borderBottom: '1px solid #1e293b',
    cursor: 'pointer',
    userSelect: 'none',
    whiteSpace: 'nowrap',
  },
  thSortableActive: {
    color: '#a5b4fc',
  },
}

// --- Helpers ---

const KIND_LABELS = {
  // RPC provider availability
  dns_resolve:          'DNS',
  tcp_connect:          'TCP',
  tls_handshake:        'TLS',
  http_control:         'HTTP-ctrl',
  https_json_rpc:       'HTTPS RPC',
  https_json_rpc_write: 'HTTPS Write',
  wss_json_rpc:         'WSS RPC',
  wss_subscribe:        'WSS Subscribe',
  // Execution layer P2P
  p2p_tcp_connect:      'P2P TCP',
  discv4_ping:          'DiscV4 UDP',
  dns_compare:          'DNS Compare',
  discv5_ping:          'DiscV5 (exec)',
  rlpx_handshake:       'RLPx Auth',
  // Consensus layer
  beacon_discv5_ping:   'DiscV5 (beacon)',
  beacon_tcp_connect:   'Beacon TCP',
  lib_p2p_handshake:    'libp2p',
  beacon_https:         'Beacon API',
}

// Probe kinds that belong to the execution P2P section.
const EXEC_P2P_KINDS = new Set([
  'p2p_tcp_connect', 'discv4_ping', 'dns_compare', 'discv5_ping', 'rlpx_handshake',
])
// Probe kinds that belong to the consensus / beacon section.
const BEACON_KINDS = new Set([
  'beacon_discv5_ping', 'beacon_tcp_connect', 'lib_p2p_handshake', 'beacon_https',
])
// Combined for any P2P section (used to exclude from RPC section).
const P2P_KINDS = new Set([...EXEC_P2P_KINDS, ...BEACON_KINDS])

/**
 * Returns { error, category } for a failed probe by inspecting its attempts.
 * Uses the last attempt that has an error (most recent retry).
 */
function getFailureInfo(r) {
  if (r.summary.ok) return { error: null, category: null }
  for (let i = r.attempts.length - 1; i >= 0; i--) {
    const a = r.attempts[i]
    if (a.error) {
      const category = a.meta?.category ?? null
      return { error: a.error, category }
    }
  }
  return { error: 'unknown failure', category: null }
}

/**
 * For dns_compare probes, returns a human-readable summary of the IP comparison
 * from the last attempt's meta, to show in the Reason column.
 */
function getDnsCompareReason(r) {
  const last = r.attempts[r.attempts.length - 1]
  if (!last) return null
  const { system_ips = [], doh_ips = [], ip_mismatch } = last.meta ?? {}
  if (last.error) return { text: last.error, category: last.meta?.category ?? null }
  if (ip_mismatch) {
    return {
      text: `IP mismatch — system: ${system_ips.join(', ')} | DoH: ${doh_ips.join(', ')}`,
      category: 'rpc_error',
    }
  }
  return null
}

/** Extracts the short provider name from a target string like "host (name)". */
function parseTarget(target) {
  const match = target.match(/\(([^)]+)\)$/)
  if (match) {
    return { name: match[1], host: target.slice(0, target.lastIndexOf(' (')).trim() }
  }
  return { name: target, host: target }
}

function fmt(ms) {
  if (ms == null) return '—'
  return ms + ' ms'
}

function duration(report) {
  const ms = report.finished_at_ms - report.started_at_ms
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`
}

/**
 * Detects the pattern: P2P TCP OK + DiscV4 UDP FAIL for the same node name.
 * Returns the list of node names where this pattern is present.
 */
function detectUdpTcpMismatch(execResults) {
  const tcpOk  = new Set(
    execResults
      .filter(r => r.kind === 'p2p_tcp_connect' && r.summary.ok)
      .map(r => parseTarget(r.target).name)
  )
  const udpFail = new Set(
    execResults
      .filter(r => r.kind === 'discv4_ping' && !r.summary.ok)
      .map(r => parseTarget(r.target).name)
  )
  return [...tcpOk].filter(n => udpFail.has(n))
}

function UdpTcpMismatchNote({ nodes }) {
  if (nodes.length === 0) return null
  return (
    <div style={{
      marginTop: 12,
      padding: '10px 14px',
      background: '#1c1f2e',
      border: '1px solid #334155',
      borderLeft: '3px solid #f59e0b',
      borderRadius: 4,
      fontSize: 12,
      color: '#94a3b8',
      lineHeight: 1.6,
    }}>
      <span style={{ color: '#fbbf24', fontWeight: 600 }}>UDP/TCP mismatch</span>
      {' '}— TCP:30303 reaches {nodes.join(', ')} but DiscV4 UDP times out.
      {' '}This is ambiguous: it may mean (1) <strong style={{ color: '#cbd5e1' }}>your local firewall
      (Windows Defender) is blocking incoming UDP responses</strong> on the ephemeral port used by
      the probe, or (2) <strong style={{ color: '#cbd5e1' }}>your router or ISP is filtering
      UDP on port 30303</strong>. In either case, an Ethereum execution node on this network
      could connect to already-known peers via TCP but would fail at discovering new ones
      through the discv4 protocol.
    </div>
  )
}

// --- Sub-components ---

function SendStatus({ send }) {
  const label = send.kind === 'sent'
    ? 'Report sent'
    : send.kind === 'skipped'
    ? 'Report not sent (local only)'
    : `Send failed: ${send.reason ?? ''}`

  return (
    <div style={{ marginBottom: 20, display: 'flex', alignItems: 'center', gap: 8, fontSize: 13 }}>
      <span style={{ color: '#64748b' }}>Report:</span>
      <span style={S.sendBadge(send.kind)}>{label}</span>
    </div>
  )
}

// Sort direction cycles: null → 'asc' → 'desc' → null
const SORT_ICONS = { null: ' ↕', asc: ' ↑', desc: ' ↓' }

// Fixed sort priority: protocol > provider > status.
// Each column can be toggled independently (null/asc/desc).
function ResultsTable({ results }) {
  const [sort, setSort] = useState({ protocol: null, provider: null, status: null })

  function toggleSort(col) {
    setSort(prev => {
      const cur = prev[col]
      const next = cur === null ? 'asc' : cur === 'asc' ? 'desc' : null
      return { ...prev, [col]: next }
    })
  }

  const sorted = useMemo(() => {
    const items = [...results]
    // Apply sorts in fixed priority order: protocol, then provider, then status.
    items.sort((a, b) => {
      const checks = [
        [sort.protocol, KIND_LABELS[a.kind] ?? a.kind,          KIND_LABELS[b.kind] ?? b.kind],
        [sort.provider, parseTarget(a.target).name,              parseTarget(b.target).name],
        [sort.status,   a.summary.ok ? 'OK' : 'FAIL',           b.summary.ok ? 'OK' : 'FAIL'],
      ]
      for (const [dir, aVal, bVal] of checks) {
        if (!dir) continue
        const cmp = aVal.localeCompare(bVal)
        if (cmp !== 0) return dir === 'asc' ? cmp : -cmp
      }
      return 0
    })
    return items
  }, [results, sort])

  function thSort(col, label) {
    const active = sort[col] !== null
    return (
      <th
        style={active ? { ...S.thSortable, ...S.thSortableActive } : S.thSortable}
        onClick={() => toggleSort(col)}
      >
        {label}{SORT_ICONS[sort[col]] ?? SORT_ICONS.null}
      </th>
    )
  }

  return (
    <table style={S.table}>
      <thead>
        <tr>
          {thSort('protocol', 'Protocol')}
          {thSort('provider', 'Provider')}
          <th style={S.th}>Target</th>
          {thSort('status', 'Status')}
          <th style={S.th}>Reason</th>
          <th style={S.thRight}>avg RTT</th>
          <th style={S.thRight}>min</th>
          <th style={S.thRight}>max</th>
        </tr>
      </thead>
      <tbody>
        {sorted.map((r) => {
          const { name, host } = parseTarget(r.target)
          const reason = r.kind === 'dns_compare'
            ? getDnsCompareReason(r)
            : (() => { const { error, category } = getFailureInfo(r); return error ? { text: error, category } : null })()
          return (
            <tr key={`${r.kind}:${r.target}`}>
              <td style={S.td}>
                <span style={S.kindTag}>{KIND_LABELS[r.kind] ?? r.kind}</span>
              </td>
              <td style={S.td}>{name}</td>
              <td style={S.td} title={host}>
                <span style={{ display: 'inline-block', maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', verticalAlign: 'bottom', color: '#64748b', fontFamily: 'ui-monospace, monospace', fontSize: 11 }}>
                  {host}
                </span>
              </td>
              <td style={S.tdRight}>
                <span style={S.okBadge(r.summary.ok)}>{r.summary.ok ? 'OK' : 'FAIL'}</span>
              </td>
              <td style={S.td}>
                {reason && <span style={S.reasonText(reason.category)}>{reason.text}</span>}
              </td>
              <td style={S.tdRight}>{fmt(r.summary.avg_rtt_ms)}</td>
              <td style={S.tdRight}>{fmt(r.summary.min_rtt_ms)}</td>
              <td style={S.tdRight}>{fmt(r.summary.max_rtt_ms)}</td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

function Summary({ results, durationStr }) {
  const total = results.length
  const ok = results.filter((r) => r.summary.ok).length
  const failed = total - ok

  return (
    <div style={S.statRow}>
      <span style={S.stat}>Probes: <span style={S.statNum}>{total}</span></span>
      <span style={S.stat}>OK: <span style={{ ...S.statNum, color: '#86efac' }}>{ok}</span></span>
      <span style={S.stat}>Failed: <span style={{ ...S.statNum, color: failed > 0 ? '#fca5a5' : '#e2e8f0' }}>{failed}</span></span>
      {durationStr && <span style={S.stat}>Duration: <span style={S.statNum}>{durationStr}</span></span>}
    </div>
  )
}

// --- Main app ---

export default function App() {
  const [sendReport, setSendReport] = useState(true)
  const [running, setRunning] = useState(false)
  const [result, setResult] = useState(null)
  const [error, setError] = useState(null)

  async function handleRun() {
    setRunning(true)
    setError(null)
    setResult(null)
    try {
      const res = await runProbes(!sendReport)
      setResult(res)
    } catch (e) {
      setError(String(e))
    } finally {
      setRunning(false)
    }
  }

  return (
    <>
      {/* Keyframe for spinner — injected once via a style tag */}
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>

      <div style={S.page}>
        <div style={S.inner}>

          {/* Header */}
          <h1 style={S.title}>Ethereum Connectivity Prober</h1>
          <p style={S.subtitle}>
            Tests RPC provider availability and Ethereum P2P network reachability from your
            location. Results can optionally be submitted as an anonymous report to help map
            network access across regions.
          </p>

          {/* Controls */}
          <div style={S.controls}>
            <label style={S.toggle}>
              <input
                type="checkbox"
                style={S.checkbox}
                checked={sendReport}
                onChange={(e) => setSendReport(e.target.checked)}
              />
              Send anonymous report
            </label>

            <button
              onClick={handleRun}
              disabled={running}
              style={running ? { ...S.button, ...S.buttonDisabled } : S.button}
            >
              {running && <span style={S.spinner} />}
              {running ? 'Running…' : 'Run probes'}
            </button>
          </div>

          {/* Error */}
          {error !== null && (
            <div style={S.errorBox}>
              <strong>Error:</strong> {error}
            </div>
          )}

          {/* Results */}
          {result !== null && (() => {
            const rpcResults    = result.report.results.filter(r => !P2P_KINDS.has(r.kind))
            const execP2pResults = result.report.results.filter(r => EXEC_P2P_KINDS.has(r.kind))
            const beaconResults  = result.report.results.filter(r => BEACON_KINDS.has(r.kind))
            return (
              <div>
                <SendStatus send={result.send} />

                <div style={S.section}>
                  <div style={S.sectionTitle}>Section 1 — RPC provider availability</div>
                  <p style={{ margin: '0 0 10px', fontSize: 12, color: '#64748b' }}>
                    Tests whether public Ethereum RPC endpoints are reachable from your location.
                    This covers the access layer used by wallets (MetaMask, etc.) and dapps.
                    Includes TLS handshake inspection, write-path censorship detection
                    (HTTPS Write sends <code>eth_sendRawTransaction</code>), and WebSocket
                    subscription availability.
                  </p>
                  <Summary results={rpcResults} durationStr={duration(result.report)} />
                  <ResultsTable results={rpcResults} />
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>Section 2 — Execution layer P2P</div>
                  <p style={{ margin: '0 0 10px', fontSize: 12, color: '#64748b' }}>
                    Tests the Ethereum execution layer P2P network. Port 30303 is used by
                    execution nodes (RLPx + DiscV4). If TCP:30303 fails while TCP:443 works, it
                    indicates selective port blocking. RLPx Auth sends an EIP-8 ECIES auth packet
                    — a connection reset (RST) means the port is open but the node rejected our
                    identity; a timeout means the port is blocked. DNS Compare checks whether your
                    local resolver returns the same addresses as Cloudflare DoH (1.1.1.1).
                  </p>
                  <Summary results={execP2pResults} />
                  <ResultsTable results={execP2pResults} />
                  <UdpTcpMismatchNote nodes={detectUdpTcpMismatch(execP2pResults)} />
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>Section 3 — Consensus layer P2P (Beacon chain)</div>
                  <p style={{ margin: '0 0 10px', fontSize: 12, color: '#64748b' }}>
                    Tests the Ethereum consensus (Beacon chain) layer. DiscV5 (beacon) pings
                    consensus boot nodes via UDP:9000. Beacon TCP and libp2p probes are derived
                    from the same ENR entries, they test whether TCP:9000 is open and whether the
                    node speaks the libp2p multistream-select protocol. A "connection refused" on
                    TCP while DiscV5 UDP works means the inbound TCP port is firewalled. Beacon
                    API checks public REST endpoints (<code>/eth/v1/node/version</code>).
                  </p>
                  <Summary results={beaconResults} />
                  <ResultsTable results={beaconResults} />
                </div>
              </div>
            )
          })()}

        </div>
      </div>
    </>
  )
}
