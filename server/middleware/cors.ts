/**
 * CORS for the desktop (Tauri) WebView only: answers preflight and adds Access-Control-* headers when the
 * request Origin is listed in PHRAPEDIA_ALLOWED_ORIGINS. A no-op for every other origin / when unset.
 */
export default defineEventHandler((event) => {
  const origin = getRequestHeader(event, 'origin')
  if (!isAllowedClientOrigin(origin)) return
  setResponseHeaders(event, {
    'Access-Control-Allow-Origin': origin as string,
    'Access-Control-Allow-Methods': 'GET, POST, PUT, PATCH, DELETE, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type, Authorization',
    'Vary': 'Origin',
  })
  if (event.method === 'OPTIONS') {
    setResponseStatus(event, 204)
    return ''
  }
})
