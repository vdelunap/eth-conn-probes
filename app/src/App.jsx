import { useState, useMemo, lazy, Suspense } from 'react'
import { runProbes } from './lib/api.js'

const MapView = lazy(() => import('./MapView.jsx'))

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
  // Amber and purple mean "the provider answered, just not with what we wanted";
  // red means we never got through.
  reasonText: (category) => {
    const colors = {
      rate_limited:  '#f59e0b',
      auth_required: '#a78bfa',
      rpc_error:     '#f97316',
      api_error:     '#f97316',
      http_error:    '#ef4444',
      network:       '#ef4444',
      dns_error:     '#ef4444',
      timeout:       '#ef4444',
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
  tabs: {
    display: 'flex',
    gap: 4,
    marginBottom: 24,
    borderBottom: '1px solid #1e293b',
  },
  tab: (active) => ({
    padding: '7px 16px',
    fontSize: 13,
    fontWeight: 600,
    color: active ? '#a5b4fc' : '#64748b',
    background: 'none',
    border: 'none',
    borderBottom: `2px solid ${active ? '#6366f1' : 'transparent'}`,
    cursor: 'pointer',
    marginBottom: -1,
  }),
  networkRow: {
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    fontSize: 13,
    color: '#94a3b8',
  },
  networkSelect: {
    padding: '5px 8px',
    fontSize: 12,
    background: '#1e293b',
    color: '#e2e8f0',
    border: '1px solid #334155',
    borderRadius: 5,
    cursor: 'pointer',
  },
  networkInput: {
    padding: '5px 8px',
    fontSize: 12,
    background: '#1e293b',
    color: '#e2e8f0',
    border: '1px solid #334155',
    borderRadius: 5,
    width: 140,
    outline: 'none',
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

const EXEC_P2P_KINDS = new Set([
  'p2p_tcp_connect', 'discv4_ping', 'dns_compare', 'discv5_ping', 'rlpx_handshake',
])
const BEACON_KINDS = new Set([
  'beacon_discv5_ping', 'beacon_tcp_connect', 'lib_p2p_handshake', 'beacon_https',
])
// Anything not in here belongs to the RPC section.
const P2P_KINDS = new Set([...EXEC_P2P_KINDS, ...BEACON_KINDS])

// Error and category of the most recent failed attempt.
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

// dns_compare passes even when the two resolvers disagree, so the Reason column
// has to dig the comparison out of meta itself.
function getDnsCompareReason(r) {
  const last = r.attempts[r.attempts.length - 1]
  if (!last) return null
  const { system_ips = [], doh_ips = [], ip_mismatch } = last.meta ?? {}
  if (last.error) return { text: last.error, category: last.meta?.category ?? null }
  if (ip_mismatch) {
    return {
      text: `IP mismatch, system: ${system_ips.join(', ')} | DoH: ${doh_ips.join(', ')}`,
      category: 'rpc_error',
    }
  }
  return null
}

// Targets are formatted as "host (name)".
function parseTarget(target) {
  const match = target.match(/\(([^)]+)\)$/)
  if (match) {
    return { name: match[1], host: target.slice(0, target.lastIndexOf(' (')).trim() }
  }
  return { name: target, host: target }
}

function fmt(ms) {
  if (ms == null) return '-'
  return ms + ' ms'
}

function duration(report) {
  const ms = report.finished_at_ms - report.started_at_ms
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`
}

// Nodes we can reach over TCP but not over DiscV4 UDP.
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
      {'. '}TCP:30303 reaches {nodes.join(', ')} but DiscV4 UDP times out. Either your
      {' '}<strong style={{ color: '#cbd5e1' }}>local firewall is dropping the UDP replies</strong>
      {' '}on the probe's ephemeral port, or your{' '}
      <strong style={{ color: '#cbd5e1' }}>router or ISP filters UDP on 30303</strong>.
      {' '}Either way, an execution node here could talk to peers it already knows but
      {' '}could not discover new ones.
    </div>
  )
}

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

// Each header cycles null → asc → desc → null.
const SORT_ICONS = { null: ' ↕', asc: ' ↑', desc: ' ↓' }

// Columns sort independently but always in the order protocol > provider > status.
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

const NETWORK_PRESETS = [
  { value: '',           label: 'Not specified' },
  { value: 'home',       label: 'Home' },
  { value: 'university', label: 'University' },
  { value: 'office',     label: 'Office' },
  { value: 'cafe',       label: 'Café / Restaurant' },
  { value: 'mobile',     label: 'Mobile (4G/5G)' },
  { value: 'vpn',        label: 'VPN' },
  { value: '__other__',  label: 'Other…' },
]

export default function App() {
  const [tab, setTab] = useState('probes')
  const [sendReport, setSendReport] = useState(true)
  const [networkPreset, setNetworkPreset] = useState('')
  const [networkCustom, setNetworkCustom] = useState('')
  const [running, setRunning] = useState(false)
  const [result, setResult] = useState(null)
  const [error, setError] = useState(null)

  const networkLabel = networkPreset === '__other__'
    ? networkCustom.trim() || null
    : networkPreset || null

  async function handleRun() {
    setRunning(true)
    setError(null)
    setResult(null)
    try {
      const res = await runProbes(!sendReport, networkLabel)
      setResult(res)
    } catch (e) {
      setError(String(e))
    } finally {
      setRunning(false)
    }
  }

  return (
    <>
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>

      <div style={S.page}>
        <div style={S.inner}>

          <h1 style={S.title}>Ethereum Connectivity Prober</h1>
          <p style={S.subtitle}>
            Tests RPC provider availability and Ethereum P2P network reachability from your
            location. Results can optionally be submitted as an anonymous report to help map
            network access across regions.
          </p>

          <div style={S.tabs}>
            <button style={S.tab(tab === 'probes')} onClick={() => setTab('probes')}>Probes</button>
            <button style={S.tab(tab === 'map')} onClick={() => setTab('map')}>Map</button>
          </div>

          {tab === 'probes' ? (<div>
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

          <div style={{ ...S.networkRow, marginBottom: 20 }}>
            <span>Network:</span>
            <select
              style={S.networkSelect}
              value={networkPreset}
              onChange={e => setNetworkPreset(e.target.value)}
            >
              {NETWORK_PRESETS.map(p => (
                <option key={p.value} value={p.value}>{p.label}</option>
              ))}
            </select>
            {networkPreset === '__other__' && (
              <input
                style={S.networkInput}
                type="text"
                placeholder="Describe your network…"
                maxLength={64}
                value={networkCustom}
                onChange={e => setNetworkCustom(e.target.value)}
              />
            )}
          </div>

          {error !== null && (
            <div style={S.errorBox}>
              <strong>Error:</strong> {error}
            </div>
          )}

          {result !== null && (() => {
            const rpcResults    = result.report.results.filter(r => !P2P_KINDS.has(r.kind))
            const execP2pResults = result.report.results.filter(r => EXEC_P2P_KINDS.has(r.kind))
            const beaconResults  = result.report.results.filter(r => BEACON_KINDS.has(r.kind))
            return (
              <div>
                <SendStatus send={result.send} />

                <div style={S.section}>
                  <div style={S.sectionTitle}>Section 1: RPC provider availability</div>
                  <p style={{ margin: '0 0 10px', fontSize: 12, color: '#64748b' }}>
                    Whether public RPC endpoints answer from here, the layer wallets and
                    dapps depend on. HTTPS Write sends an <code>eth_sendRawTransaction</code>
                    {' '}to check the write path separately from the read path.
                  </p>
                  <Summary results={rpcResults} durationStr={duration(result.report)} />
                  <ResultsTable results={rpcResults} />
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>Section 2: Execution layer P2P</div>
                  <p style={{ margin: '0 0 10px', fontSize: 12, color: '#64748b' }}>
                    Execution nodes talk RLPx and DiscV4 on port 30303. If 443 works and 30303
                    doesn't, something is blocking that port specifically. RLPx Auth sends an
                    EIP-8 auth packet: a reset means the node just rejected our identity, a
                    timeout means the packet never arrived. DNS Compare puts your resolver
                    side by side with Cloudflare DoH.
                  </p>
                  <Summary results={execP2pResults} />
                  <ResultsTable results={execP2pResults} />
                  <UdpTcpMismatchNote nodes={detectUdpTcpMismatch(execP2pResults)} />
                </div>

                <div style={S.section}>
                  <div style={S.sectionTitle}>Section 3: Consensus layer P2P (Beacon chain)</div>
                  <p style={{ margin: '0 0 10px', fontSize: 12, color: '#64748b' }}>
                    DiscV5 pings consensus boot nodes on UDP:9000. The Beacon TCP and libp2p
                    targets are live peers pulled from <code>/eth/v1/node/peers</code> at run
                    time, outbound ones only, so we know a beacon node already dialled them
                    successfully and their addresses are publicly routable.
                  </p>
                  <Summary results={beaconResults} />
                  <ResultsTable results={beaconResults} />
                </div>
              </div>
            )
          })()}
          </div>) : (
            <Suspense fallback={<div style={{ color: '#64748b', fontSize: 13 }}>Loading map…</div>}>
              <MapView />
            </Suspense>
          )}

        </div>
      </div>
    </>
  )
}
