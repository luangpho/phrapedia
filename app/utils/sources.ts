/** Marker href of a primary-source file inside rendered markdown; resolved to a Blob URL after rendering. */
export const SOURCE_HREF_PREFIX = '#phra-src/'

/** Encode a sources-relative path (segment-wise). */
export function encodeSourcePath(path: string): string {
  return path.replace(/^\/+/, '').split('/').map(encodeURIComponent).join('/')
}

/** Sources-relative path from a marker href made by `rewriteSourceHref`; null for any other href. */
export function sourcePathFromHref(href: string | null | undefined): string | null {
  if (!href || !href.startsWith(SOURCE_HREF_PREFIX)) return null
  try {
    return decodeURIComponent(href.slice(SOURCE_HREF_PREFIX.length))
  } catch {
    return null
  }
}

export function isExternalUrl(v: string): boolean {
  return /^(https?:)?\/\//i.test(v) || /^(mailto|data):/i.test(v)
}

/**
 * Rewrite a relative image/link path found in a record body to a source-file marker href (see SOURCE_HREF_PREFIX).
 * `sources/x.jpg` -> `#phra-src/x.jpg`, `images/x.jpg` -> `#phra-src/images/x.jpg`.
 * Absolute (`/...`), external, anchor and data URLs are returned unchanged.
 */
export function rewriteSourceHref(href: string): string {
  if (!href || isExternalUrl(href) || href.startsWith('/') || href.startsWith('#')) return href
  let p = href.replace(/^\.\//, '')
  if (p.startsWith('sources/')) p = p.slice('sources/'.length)
  try {
    p = decodeURI(p)
  } catch { /* keep as-is */ }
  return SOURCE_HREF_PREFIX + encodeSourcePath(p)
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}
