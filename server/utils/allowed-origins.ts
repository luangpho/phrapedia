/**
 * Extra browser origins allowed to call the API cross-origin. Used by the desktop (Tauri) app, whose WebView
 * (tauri://localhost) is not same-origin with the 127.0.0.1 sidecar. Comma separated list in
 * PHRAPEDIA_ALLOWED_ORIGINS; empty / unset in CLI and Render runs, so nothing changes there.
 */
export function allowedClientOrigins(): string[] {
  return (process.env.PHRAPEDIA_ALLOWED_ORIGINS ?? '').split(',').map(s => s.trim()).filter(Boolean)
}

export function isAllowedClientOrigin(origin: string | undefined): boolean {
  return Boolean(origin) && allowedClientOrigins().includes(origin as string)
}
