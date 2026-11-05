/** Encode a sources-relative path for /api/sources/file/<path> (segment-wise). */
export function encodeSourcePath(path: string): string {
  return path.replace(/^\/+/, '').split('/').map(encodeURIComponent).join('/')
}

export function sourceFileUrl(path: string, raw = true): string {
  return `/api/sources/file/${encodeSourcePath(path)}${raw ? '?raw=1' : ''}`
}

export function isExternalUrl(v: string): boolean {
  return /^(https?:)?\/\//i.test(v) || /^(mailto|data):/i.test(v)
}

/**
 * Rewrite a relative image/link path found in a record body to the sources file endpoint.
 * `sources/x.jpg` -> `/api/sources/file/x.jpg?raw=1`, `images/x.jpg` -> `/api/sources/file/images/x.jpg?raw=1`.
 * Absolute (`/...`), external, anchor and data URLs are returned unchanged.
 */
export function rewriteSourceHref(href: string): string {
  if (!href || isExternalUrl(href) || href.startsWith('/') || href.startsWith('#')) return href
  let p = href.replace(/^\.\//, '')
  if (p.startsWith('sources/')) p = p.slice('sources/'.length)
  try {
    p = decodeURI(p)
  } catch { /* keep as-is */ }
  return sourceFileUrl(p, true)
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}
