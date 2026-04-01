import { invoke } from '@tauri-apps/api/core'

/**
 * Runs all probes and optionally sends the report to the server.
 *
 * @param {boolean} noSend - if true, the report is kept local and not sent.
 * @returns {Promise<{ report: Report, send: { kind: string, reason?: string } }>}
 */
export async function runProbes(noSend) {
  const json = await invoke('run_probes', { noSend })
  return JSON.parse(json)
}
