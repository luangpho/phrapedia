import { createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'

/**
 * Serve a primary-source file. `?raw=1` streams the binary (images/PDF for <img>/<iframe>);
 * otherwise returns JSON SourceContent (text for markdown, base64 for images).
 */
export default defineEventHandler(async (event) => {
  const rel = decodeURIComponent(getRouterParam(event, 'path') ?? '')
  const q = getQuery(event)
  if (q.raw) {
    const { full, mimeType } = await sourceAbsolutePath(rel)
    let st
    try {
      st = await stat(full)
    } catch {
      throw createError({ statusCode: 404, statusMessage: 'Not found' })
    }
    if (!st.isFile()) throw createError({ statusCode: 404, statusMessage: 'Not found' })
    setResponseHeader(event, 'Content-Type', mimeType ?? 'application/octet-stream')
    setResponseHeader(event, 'Content-Length', st.size)
    setResponseHeader(event, 'Cache-Control', 'private, max-age=60')
    return sendStream(event, createReadStream(full))
  }
  return readSource(rel, { maxChars: typeof q.maxChars === 'string' ? Number(q.maxChars) : undefined })
})
