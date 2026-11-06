/**
 * Cross-site request protection for the JSON API.
 * The app is local-first and has no login, so a malicious web page could otherwise make the user's browser
 * fire state-changing requests (save/delete records, run the AI, git push) at http://localhost:3000.
 * Rules for mutating requests to /api/**:
 *  - the body must be JSON (HTML forms cannot send application/json without a CORS preflight), and
 *  - when the browser sends Origin / Sec-Fetch-Site, the request must be same-origin.
 * The MCP endpoint (/mcp) is excluded: the toolkit performs its own Origin validation.
 */
const MUTATING = new Set(['POST', 'PUT', 'PATCH', 'DELETE'])

export default defineEventHandler((event) => {
  const path = event.path.split('?')[0] ?? ''
  if (!path.startsWith('/api/')) return
  if (!MUTATING.has(event.method)) return

  const contentType = (getRequestHeader(event, 'content-type') ?? '').toLowerCase()
  if (contentType && !contentType.startsWith('application/json')) {
    throw createError({ statusCode: 415, statusMessage: 'Unsupported Media Type', message: 'API は application/json のみ受け付けます' })
  }

  // Desktop app WebView (explicitly allowlisted origin, JSON-only enforced above): cross-origin by design.
  if (isAllowedClientOrigin(getRequestHeader(event, 'origin'))) return

  const fetchSite = getRequestHeader(event, 'sec-fetch-site')
  if (fetchSite && fetchSite !== 'same-origin' && fetchSite !== 'none') {
    throw createError({ statusCode: 403, statusMessage: 'Forbidden', message: 'クロスサイトからのリクエストは拒否されました' })
  }

  const origin = getRequestHeader(event, 'origin')
  if (origin) {
    const host = getRequestHeader(event, 'host')
    let originHost: string | null = null
    try {
      originHost = new URL(origin).host
    } catch {
      originHost = null
    }
    if (!host || originHost !== host) {
      throw createError({ statusCode: 403, statusMessage: 'Forbidden', message: 'Origin が一致しないためリクエストを拒否しました' })
    }
  }
})
