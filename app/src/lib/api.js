import { invoke } from '@tauri-apps/api/core'

export async function runProbes(noSend, networkLabel) {
  const json = await invoke('run_probes', {
    noSend,
    networkLabel: networkLabel || null,
  })
  return JSON.parse(json)
}

export async function getGeoReports(kinds = []) {
  const json = await invoke('get_geo_reports', { kinds })
  return JSON.parse(json)
}