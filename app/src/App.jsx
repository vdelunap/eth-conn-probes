import React, { useMemo, useState } from 'react'
import { defaultConfigToml } from './lib/defaults.js'
import { loadConfig, runPlanToml, saveConfig } from './lib/api.js'

function App() {
  const initial = useMemo(function getInitial() {
    const stored = loadConfig()
    return stored.length > 0 ? stored : defaultConfigToml
  }, [])

  const [configToml, setConfigToml] = useState(initial)
  const [noSend, setNoSend] = useState(false)
  const [running, setRunning] = useState(false)
  const [result, setResult] = useState(null)
  const [error, setError] = useState(null)

  async function onRun() {
    setRunning(true)
    setError(null)
    setResult(null)
    saveConfig(configToml)

    try {
      const res = await runPlanToml(configToml, noSend)
      setResult(res)
    } catch (e) {
      setError(String(e))
    } finally {
      setRunning(false)
    }
  }

  return (
    <div style={{ padding: 12, fontFamily: 'system-ui, sans-serif', maxWidth: 1100 }}>
      <h2 style={{ margin: 0 }}>eth-prober</h2>
      <p style={{ marginTop: 6 }}>
        Edit TOML, run probes, optionally send report to 146.146.146.146:8080.
      </p>

      <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
        <label style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <input
            type='checkbox'
            checked={noSend}
            onChange={(e) => setNoSend(e.target.checked)}
          />
          Do not send report (local only)
        </label>

        <button onClick={onRun} disabled={running} style={{ padding: '6px 10px' }}>
          {running ? 'Running…' : 'Run probes'}
        </button>
      </div>

      <div style={{ marginTop: 12 }}>
        <textarea
          value={configToml}
          onChange={(e) => setConfigToml(e.target.value)}
          spellCheck={false}
          style={{
            width: '100%',
            height: 260,
            fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
            fontSize: 12
          }}
        />
      </div>

      {error !== null ? <pre style={{ marginTop: 12, color: 'crimson' }}>{error}</pre> : null}

      {result !== null ? (
        <div style={{ marginTop: 12 }}>
          <h3 style={{ marginBottom: 6 }}>Summary</h3>
          <p style={{ marginTop: 0 }}>
            Send status:{' '}
            {result.send?.kind === 'sent'
              ? 'sent'
              : result.send?.kind === 'queued'
                ? `queued (${result.send.reason})`
                : 'unknown'}
          </p>

          <table style={{ width: '100%', borderCollapse: 'collapse' }}>
            <thead>
              <tr>
                <th align='left'>Probe</th>
                <th align='left'>Target</th>
                <th align='right'>OK</th>
                <th align='right'>avg ms</th>
                <th align='right'>min ms</th>
                <th align='right'>max ms</th>
              </tr>
            </thead>
            <tbody>
              {result.report.results.map(function row(r) {
                return (
                  <tr key={`${r.kind}:${r.target}`}>
                    <td style={{ padding: '6px 4px' }}>{r.kind}</td>
                    <td style={{ padding: '6px 4px' }}>{r.target}</td>
                    <td align='right' style={{ padding: '6px 4px' }}>
                      {r.summary.ok ? 'yes' : 'no'}
                    </td>
                    <td align='right' style={{ padding: '6px 4px' }}>
                      {r.summary.avg_rtt_ms ?? '-'}
                    </td>
                    <td align='right' style={{ padding: '6px 4px' }}>
                      {r.summary.min_rtt_ms ?? '-'}
                    </td>
                    <td align='right' style={{ padding: '6px 4px' }}>
                      {r.summary.max_rtt_ms ?? '-'}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>

          <h3 style={{ marginTop: 12, marginBottom: 6 }}>Raw JSON</h3>
          <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12 }}>
            {JSON.stringify(result, null, 2)}
          </pre>
        </div>
      ) : null}
    </div>
  )
}

export default App
