import { timingSafeEqual } from 'node:crypto'

/**
 * Optional HTTP Basic authentication for deployments reachable from the internet (e.g. Render).
 * Enabled when NUXT_BASIC_AUTH_USER and NUXT_BASIC_AUTH_PASSWORD are both set; a no-op otherwise.
 * /api/health stays open for the platform health check. MCP clients send the same `Authorization: Basic` header.
 */
function safeEqual(a: string, b: string): boolean {
  const ab = Buffer.from(a)
  const bb = Buffer.from(b)
  return ab.length === bb.length && timingSafeEqual(ab, bb)
}

export default defineEventHandler((event) => {
  const cfg = useRuntimeConfig(event)
  const user = String(cfg.basicAuthUser || '')
  const password = String(cfg.basicAuthPassword || '')
  if (!user || !password) return

  const path = event.path.split('?')[0] ?? ''
  if (path === '/api/health') return

  const header = getRequestHeader(event, 'authorization') ?? ''
  if (header.startsWith('Basic ')) {
    const decoded = Buffer.from(header.slice(6), 'base64').toString('utf8')
    const idx = decoded.indexOf(':')
    if (idx > 0 && safeEqual(decoded.slice(0, idx), user) && safeEqual(decoded.slice(idx + 1), password)) return
  }
  setResponseHeader(event, 'WWW-Authenticate', 'Basic realm="Phrapedia", charset="UTF-8"')
  throw createError({ statusCode: 401, statusMessage: 'Unauthorized', message: '認証が必要です' })
})
