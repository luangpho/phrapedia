/**
 * Build a URL-safe slug. Latin text is lower-cased and dashed; Thai/Japanese text is kept
 * as-is (Nuxt/vue-router encodes it), but whitespace and unsafe characters are normalised.
 * Returns an empty string when nothing usable remains (caller should fall back to an id).
 */
export function slugify(input: string): string {
  const s = (input ?? '')
    .normalize('NFKC')
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, '-')
    .replace(/[\/\\?#%&=+:;,"'`<>|{}\[\]()!*^$@~]+/g, '')
    .replace(/\.+/g, '.')
    .replace(/-+/g, '-')
    .replace(/^[-.]+|[-.]+$/g, '')
  return s
}

export function isValidSlug(slug: string): boolean {
  return slug.length > 0 && slug.length <= 120 && !slug.includes('/') && !slug.includes('\\') && slug !== '.' && slug !== '..' && !slug.startsWith('.')
}

/** Short random id (base36) for fallback slugs and message ids. */
export function shortId(len = 8): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz0123456789'
  let out = ''
  const bytes = globalThis.crypto?.getRandomValues?.(new Uint8Array(len))
  for (let i = 0; i < len; i++) {
    const n = bytes ? bytes[i]! : Math.floor(Math.random() * 256)
    out += alphabet[n % alphabet.length]
  }
  return out
}
