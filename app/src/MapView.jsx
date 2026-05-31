import { useEffect, useRef, useState, useCallback } from 'react'
import maplibregl from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { getGeoReports } from './lib/api.js'

const PROBE_SECTIONS = [
  {
    id: 'rpc',
    label: 'RPC Providers',
    kinds: [
      { id: 'dns_resolve',          label: 'DNS' },
      { id: 'tcp_connect',          label: 'TCP' },
      { id: 'tls_handshake',        label: 'TLS' },
      { id: 'http_control',         label: 'HTTP ctrl' },
      { id: 'https_json_rpc',       label: 'HTTPS RPC' },
      { id: 'https_json_rpc_write', label: 'HTTPS Write' },
      { id: 'wss_json_rpc',         label: 'WSS RPC' },
      { id: 'wss_subscribe',        label: 'WSS Sub' },
    ],
  },
  {
    id: 'p2p',
    label: 'Execution P2P',
    kinds: [
      { id: 'p2p_tcp_connect', label: 'P2P TCP' },
      { id: 'discv4_ping',     label: 'DiscV4 UDP' },
      { id: 'discv5_ping',     label: 'DiscV5 (exec)' },
      { id: 'rlpx_handshake', label: 'RLPx Auth' },
      { id: 'dns_compare',    label: 'DNS Compare' },
    ],
  },
  {
    id: 'consensus',
    label: 'Consensus',
    kinds: [
      { id: 'beacon_discv5_ping', label: 'DiscV5 (beacon)' },
      { id: 'beacon_tcp_connect', label: 'Beacon TCP' },
      { id: 'lib_p2p_handshake',  label: 'libp2p' },
      { id: 'beacon_https',       label: 'Beacon API' },
    ],
  },
]

function FilterDropdown({ selected, onChange }) {
  const [open, setOpen] = useState(false)
  const ref = useRef(null)

  useEffect(() => {
    function handleClick(e) {
      if (ref.current && !ref.current.contains(e.target)) setOpen(false)
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [])

  function toggleKind(id) {
    onChange(
      selected.includes(id)
        ? selected.filter(k => k !== id)
        : [...selected, id]
    )
  }

  function toggleSection(sectionKinds) {
    const ids = sectionKinds.map(k => k.id)
    const allOn = ids.every(id => selected.includes(id))
    onChange(
      allOn
        ? selected.filter(id => !ids.includes(id))
        : [...new Set([...selected, ...ids])]
    )
  }

  function selectAll() { onChange([]) }

  const label = selected.length === 0
    ? 'All probes'
    : `${selected.length} probe type${selected.length > 1 ? 's' : ''}`

  return (
    <div ref={ref} style={{ position: 'relative', display: 'inline-block' }}>
      <button
        onClick={() => setOpen(o => !o)}
        style={{
          padding: '5px 12px',
          fontSize: 12,
          fontWeight: 600,
          background: selected.length > 0 ? '#312e81' : '#1e293b',
          color: selected.length > 0 ? '#a5b4fc' : '#94a3b8',
          border: `1px solid ${selected.length > 0 ? '#4338ca' : '#334155'}`,
          borderRadius: 5,
          cursor: 'pointer',
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        {label} <span style={{ opacity: 0.7 }}>{open ? '▲' : '▼'}</span>
      </button>

      {open && (
        <div style={{
          position: 'absolute',
          top: 'calc(100% + 6px)',
          left: 0,
          zIndex: 1000,
          background: '#1e293b',
          border: '1px solid #334155',
          borderRadius: 6,
          padding: '8px 0',
          minWidth: 220,
          boxShadow: '0 8px 24px rgba(0,0,0,0.5)',
        }}>
          <div
            onClick={selectAll}
            style={{
              padding: '4px 12px',
              fontSize: 12,
              cursor: 'pointer',
              color: selected.length === 0 ? '#a5b4fc' : '#64748b',
              fontWeight: selected.length === 0 ? 600 : 400,
            }}
          >
            ✓ All probes
          </div>
          <div style={{ borderTop: '1px solid #334155', margin: '4px 0' }} />
          {PROBE_SECTIONS.map(section => {
            const ids = section.kinds.map(k => k.id)
            const allOn = ids.every(id => selected.includes(id))
            const someOn = ids.some(id => selected.includes(id))
            return (
              <div key={section.id}>
                <div
                  onClick={() => toggleSection(section.kinds)}
                  style={{
                    padding: '4px 12px',
                    fontSize: 11,
                    fontWeight: 700,
                    color: someOn ? '#94a3b8' : '#475569',
                    letterSpacing: '0.06em',
                    textTransform: 'uppercase',
                    cursor: 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                  }}
                >
                  <span style={{
                    display: 'inline-block',
                    width: 10,
                    height: 10,
                    border: '1px solid #475569',
                    borderRadius: 2,
                    background: allOn ? '#6366f1' : someOn ? '#312e81' : 'transparent',
                    flexShrink: 0,
                  }} />
                  {section.label}
                </div>
                {section.kinds.map(kind => (
                  <div
                    key={kind.id}
                    onClick={() => toggleKind(kind.id)}
                    style={{
                      padding: '3px 12px 3px 24px',
                      fontSize: 12,
                      cursor: 'pointer',
                      color: selected.includes(kind.id) ? '#c7d2fe' : '#64748b',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 6,
                    }}
                  >
                    <span style={{
                      display: 'inline-block',
                      width: 10,
                      height: 10,
                      border: '1px solid #475569',
                      borderRadius: 2,
                      background: selected.includes(kind.id) ? '#6366f1' : 'transparent',
                      flexShrink: 0,
                    }} />
                    {kind.label}
                  </div>
                ))}
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

export default function MapView() {
  const containerRef = useRef(null)
  const mapRef = useRef(null)
  const popupRef = useRef(null)

  const [selectedKinds, setSelectedKinds] = useState([])
  const [status, setStatus] = useState('idle') // idle | loading | error
  const [errorMsg, setErrorMsg] = useState(null)
  const [pointCount, setPointCount] = useState(null)
  const [mapReady, setMapReady] = useState(false)

  const loadData = useCallback(async (kinds) => {
    if (!mapRef.current) return
    setStatus('loading')
    setErrorMsg(null)
    try {
      const geojson = await getGeoReports(kinds)
      const source = mapRef.current.getSource('reports')
      if (source) source.setData(geojson)
      setPointCount(geojson.features?.length ?? 0)
      setStatus('idle')
    } catch (e) {
      setStatus('error')
      setErrorMsg(String(e))
    }
  }, [])

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return

    const map = new maplibregl.Map({
      container: containerRef.current,
      style: 'https://tiles.openfreemap.org/styles/liberty',
      center: [0, 20],
      zoom: 1.5,
      attributionControl: true,
    })

    map.addControl(new maplibregl.NavigationControl(), 'top-right')

    map.on('load', () => {
      map.addSource('reports', {
        type: 'geojson',
        data: { type: 'FeatureCollection', features: [] },
      })

      map.addLayer({
        id: 'reports-layer',
        type: 'circle',
        source: 'reports',
        paint: {
          'circle-radius': [
            'interpolate', ['linear'], ['zoom'],
            1, 4,
            6, 7,
            10, 10,
          ],
          'circle-color': [
            'interpolate', ['linear'],
            ['/', ['get', 'ok_count'], ['max', 1, ['get', 'total']]],
            0,   '#ef4444',
            0.5, '#f59e0b',
            1,   '#22c55e',
          ],
          'circle-opacity': 0.85,
          'circle-stroke-width': 1,
          'circle-stroke-color': '#0f172a',
        },
      })

      mapRef.current = map
      setMapReady(true)
    })

    map.on('click', 'reports-layer', (e) => {
      const f = e.features[0]
      if (!f) return
      const p = f.properties
      const pct = p.total > 0 ? Math.round((p.ok_count / p.total) * 100) : 0
      const location = [p.city_name, p.country_name].filter(Boolean).join(', ') || 'Unknown location'
      const when = new Date(p.received_at).toLocaleString()
      const netLabel = p.network_label ? `<div style="margin-top:4px;color:#94a3b8">Network: <b style="color:#e2e8f0">${p.network_label}</b></div>` : ''

      if (popupRef.current) popupRef.current.remove()
      popupRef.current = new maplibregl.Popup({ offset: 10, closeButton: true })
        .setLngLat(e.lngLat)
        .setHTML(`
          <div style="font-family:system-ui,sans-serif;font-size:13px;color:#e2e8f0;padding:2px 4px;min-width:180px">
            <div style="font-weight:700;margin-bottom:6px">${location}</div>
            <div>Probes: <b>${p.ok_count}/${p.total} OK</b> <span style="color:${pct>=80?'#86efac':pct>=50?'#fbbf24':'#fca5a5'}">(${pct}%)</span></div>
            ${netLabel}
            <div style="margin-top:4px;font-size:11px;color:#64748b">${when}</div>
          </div>
        `)
        .addTo(map)
    })

    map.on('mouseenter', 'reports-layer', () => { map.getCanvas().style.cursor = 'pointer' })
    map.on('mouseleave', 'reports-layer', () => { map.getCanvas().style.cursor = '' })

    return () => {
      map.remove()
      mapRef.current = null
    }
  }, [])

  useEffect(() => {
    if (mapReady) loadData(selectedKinds)
  }, [mapReady, selectedKinds, loadData])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
        <span style={{ fontSize: 12, color: '#64748b' }}>Filter by probe:</span>
        <FilterDropdown selected={selectedKinds} onChange={setSelectedKinds} />
        {status === 'loading' && (
          <span style={{ fontSize: 12, color: '#64748b' }}>Loading…</span>
        )}
        {status === 'idle' && pointCount !== null && (
          <span style={{ fontSize: 12, color: '#64748b' }}>
            {pointCount} report{pointCount !== 1 ? 's' : ''}
          </span>
        )}
        {status !== 'loading' && (
          <button
            onClick={() => loadData(selectedKinds)}
            style={{
              padding: '4px 10px',
              fontSize: 11,
              background: '#1e293b',
              color: '#94a3b8',
              border: '1px solid #334155',
              borderRadius: 4,
              cursor: 'pointer',
            }}
          >
            Refresh
          </button>
        )}
      </div>

      {status === 'error' && (
        <div style={{
          padding: '8px 12px',
          background: '#450a0a',
          border: '1px solid #7f1d1d',
          borderRadius: 5,
          color: '#fca5a5',
          fontSize: 12,
        }}>
          Could not load map data: {errorMsg}
        </div>
      )}

      <div
        ref={containerRef}
        style={{
          width: '100%',
          height: 520,
          borderRadius: 8,
          overflow: 'hidden',
          border: '1px solid #1e293b',
        }}
      />

      <div style={{ display: 'flex', gap: 16, alignItems: 'center', fontSize: 11, color: '#64748b' }}>
        <span>Probe success rate:</span>
        <span>
          <span style={{ color: '#ef4444' }}>●</span> 0%
        </span>
        <span>
          <span style={{ color: '#f59e0b' }}>●</span> 50%
        </span>
        <span>
          <span style={{ color: '#22c55e' }}>●</span> 100%
        </span>
        <span style={{ marginLeft: 8 }}>Click a point for details</span>
      </div>
    </div>
  )
}