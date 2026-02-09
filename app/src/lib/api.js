import { invoke } from '@tauri-apps/api/core'

export function loadConfig() {
  const v = localStorage.getItem('eth-prober.configToml')
  return v ?? ''
}

export function saveConfig(v) {
  localStorage.setItem('eth-prober.configToml', v)
}

/**
 * Calls the Tauri Rust backend command.
 * Rust returns JSON string; we parse it here.
 */
export async function runPlanToml(configToml, noSend) {
  const json = await invoke('run_plan_toml', { configToml, noSend })
  return JSON.parse(json)
}
