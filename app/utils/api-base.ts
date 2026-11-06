/**
 * Base URL of the API server. Empty in the browser / CLI (same origin). The desktop (Tauri) app sets it to the
 * local sidecar, e.g. http://127.0.0.1:41234 (see app/plugins/tauri.client.ts).
 */
let apiBase = ''

export function setApiBase(url: string | null | undefined): void {
  apiBase = (url ?? '').replace(/\/+$/, '')
}

/** Prefix an absolute-path API URL (/api/..., /mcp) with the API base. */
export function apiUrl(path: string): string {
  return apiBase && /^\/(api\/|mcp)/.test(path) ? `${apiBase}${path}` : path
}
