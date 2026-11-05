import YAML from 'yaml'

const FM_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/

export interface ParsedMarkdown<T = Record<string, unknown>> {
  data: T
  body: string
}

/** Split `--- yaml ---` frontmatter from the body. Missing frontmatter yields `{}`. */
export function parseFrontmatter<T = Record<string, unknown>>(raw: string): ParsedMarkdown<T> {
  const text = raw.replace(/^﻿/, '')
  const m = FM_RE.exec(text)
  if (!m) return { data: {} as T, body: text }
  let data: unknown
  try {
    data = YAML.parse(m[1]!) ?? {}
  } catch (err) {
    throw createError({ statusCode: 400, statusMessage: 'Invalid frontmatter', message: `YAML frontmatter could not be parsed: ${(err as Error).message}` })
  }
  if (typeof data !== 'object' || data === null || Array.isArray(data)) {
    throw createError({ statusCode: 400, statusMessage: 'Invalid frontmatter', message: 'Frontmatter must be a YAML mapping' })
  }
  return { data: normaliseDates(data as Record<string, unknown>) as T, body: m[2] ?? '' }
}

/** Serialise frontmatter + body. Dates stay strings; unicode is preserved; long lines are not folded. */
export function stringifyFrontmatter(data: Record<string, unknown>, body: string): string {
  const clean: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(data)) {
    if (v === undefined) continue
    clean[k] = v
  }
  const yaml = YAML.stringify(clean, { lineWidth: 0, defaultStringType: 'PLAIN', defaultKeyType: 'PLAIN' }).trimEnd()
  const trimmedBody = body.replace(/^\s*\n/, '')
  return `---\n${yaml}\n---\n\n${trimmedBody.endsWith('\n') ? trimmedBody : trimmedBody + '\n'}`
}

/** YAML turns unquoted dates into Date objects; records keep them as ISO/plain strings. */
function normaliseDates(obj: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(obj)) {
    out[k] = v instanceof Date ? v.toISOString().slice(0, 10) : v
  }
  return out
}
