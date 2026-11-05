import { readdir, readFile, writeFile, mkdir, stat, unlink, rename } from 'node:fs/promises'
import { join, basename } from 'node:path'
import type { EntityFrontmatter, EntityKind, EntityRecord, EntitySummary, EntityStatus } from '../../shared/types/entity'
import { ENTITY_KINDS } from '../../shared/types/entity'
import { isValidSlug, slugify, shortId } from '../../shared/utils/slug'

export function assertKind(kind: string | undefined): EntityKind {
  if (!kind || !ENTITY_KINDS.includes(kind as EntityKind)) {
    throw createError({ statusCode: 400, statusMessage: 'Unknown kind', message: `Unknown entity kind: ${kind}` })
  }
  return kind as EntityKind
}

export function assertSlug(slug: string | undefined): string {
  const s = decodeURIComponent(slug ?? '')
  if (!isValidSlug(s)) {
    throw createError({ statusCode: 400, statusMessage: 'Invalid slug', message: `Invalid slug: ${slug}` })
  }
  return s
}

export async function kindDir(kind: EntityKind): Promise<string> {
  const dir = join(await dataDir(), kind)
  await mkdir(dir, { recursive: true })
  return dir
}

export async function entityPath(kind: EntityKind, slug: string): Promise<string> {
  return join(await kindDir(kind), `${slug}.md`)
}

function toStatus(v: unknown): EntityStatus {
  return v === 'verified' || v === 'reviewed' ? v : 'draft'
}

function relPath(kind: EntityKind, slug: string): string {
  return `data/${kind}/${slug}.md`
}

function toRecord(kind: EntityKind, slug: string, raw: string, mtime?: Date): EntityRecord {
  const { data, body } = parseFrontmatter<Partial<EntityFrontmatter>>(raw)
  const fm: EntityFrontmatter = {
    ...data,
    slug,
    kind,
    name: typeof data.name === 'string' && data.name ? data.name : titleFromBody(body) || slug,
  }
  return {
    kind,
    slug,
    name: fm.name,
    name_th: typeof fm.name_th === 'string' ? fm.name_th : undefined,
    name_en: typeof fm.name_en === 'string' ? fm.name_en : undefined,
    status: toStatus(fm.status),
    updatedAt: typeof fm.updated_at === 'string' ? fm.updated_at : mtime?.toISOString(),
    frontmatter: fm,
    body,
    raw,
    path: relPath(kind, slug),
  }
}

function titleFromBody(body: string): string | undefined {
  const m = /^#\s+(.+)$/m.exec(body)
  return m?.[1]?.trim()
}

export async function listEntities(kind: EntityKind): Promise<EntitySummary[]> {
  const dir = await kindDir(kind)
  const files = (await readdir(dir)).filter(f => f.endsWith('.md') && !f.startsWith('.') && f.toLowerCase() !== 'readme.md')
  const out: EntitySummary[] = []
  for (const f of files) {
    const slug = basename(f, '.md')
    try {
      const full = join(dir, f)
      const [raw, st] = await Promise.all([readFile(full, 'utf8'), stat(full)])
      const rec = toRecord(kind, slug, raw, st.mtime)
      out.push({ kind, slug, name: rec.name, name_th: rec.name_th, name_en: rec.name_en, status: rec.status, updatedAt: rec.updatedAt })
    } catch {
      out.push({ kind, slug, name: slug, status: 'draft' })
    }
  }
  return out.sort((a, b) => a.name.localeCompare(b.name, 'ja'))
}

export async function getEntity(kind: EntityKind, slug: string): Promise<EntityRecord | null> {
  const full = await entityPath(kind, slug)
  try {
    const [raw, st] = await Promise.all([readFile(full, 'utf8'), stat(full)])
    return toRecord(kind, slug, raw, st.mtime)
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') return null
    throw err
  }
}

export async function entityExists(kind: EntityKind, slug: string): Promise<boolean> {
  try {
    await stat(await entityPath(kind, slug))
    return true
  } catch {
    return false
  }
}

/**
 * Derive a slug for a new record: explicit frontmatter slug > name_en > name > random id.
 * Ensures uniqueness by appending -2, -3, ...
 */
export async function deriveSlug(kind: EntityKind, fm: Partial<EntityFrontmatter>): Promise<string> {
  const candidates = [fm.slug, fm.name_en, fm.name, fm.name_th].filter((v): v is string => typeof v === 'string' && v.trim().length > 0)
  let base = ''
  for (const c of candidates) {
    const s = slugify(c)
    if (isValidSlug(s)) { base = s; break }
  }
  if (!base) base = `${kind.slice(0, -1)}-${shortId(6)}`
  let slug = base
  let n = 2
  while (await entityExists(kind, slug)) slug = `${base}-${n++}`
  return slug
}

export interface WriteEntityOptions {
  /** When true, fail if the file already exists */
  mustNotExist?: boolean
  /** When true, fail if the file does not exist */
  mustExist?: boolean
}

/**
 * Normalise and write a record. Returns the saved record.
 * - slug/kind in frontmatter are forced to match the path
 * - created_at / updated_at are maintained
 */
export async function writeEntity(kind: EntityKind, slug: string, raw: string, opts: WriteEntityOptions = {}): Promise<EntityRecord> {
  const exists = await entityExists(kind, slug)
  if (opts.mustNotExist && exists) throw createError({ statusCode: 409, statusMessage: 'Exists', message: `${kind}/${slug} already exists` })
  if (opts.mustExist && !exists) throw createError({ statusCode: 404, statusMessage: 'Not found', message: `${kind}/${slug} not found` })
  const { data, body } = parseFrontmatter<Partial<EntityFrontmatter>>(raw)
  const now = new Date().toISOString()
  const previous = exists ? await getEntity(kind, slug) : null
  const fm: EntityFrontmatter = {
    ...data,
    slug,
    kind,
    name: typeof data.name === 'string' && data.name ? data.name : titleFromBody(body) || slug,
    status: toStatus(data.status),
    created_at: (typeof data.created_at === 'string' && data.created_at) || previous?.frontmatter.created_at || now,
    updated_at: now,
  }
  const content = stringifyFrontmatter(fm, body)
  const full = await entityPath(kind, slug)
  await writeFile(full, content, 'utf8')
  return toRecord(kind, slug, content, new Date(now))
}

export async function deleteEntity(kind: EntityKind, slug: string): Promise<void> {
  const full = await entityPath(kind, slug)
  try {
    await unlink(full)
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') throw createError({ statusCode: 404, statusMessage: 'Not found' })
    throw err
  }
}

export async function renameEntity(kind: EntityKind, from: string, to: string): Promise<EntityRecord> {
  if (await entityExists(kind, to)) throw createError({ statusCode: 409, statusMessage: 'Exists', message: `${kind}/${to} already exists` })
  const rec = await getEntity(kind, from)
  if (!rec) throw createError({ statusCode: 404, statusMessage: 'Not found' })
  await rename(await entityPath(kind, from), await entityPath(kind, to))
  return writeEntity(kind, to, rec.raw, { mustExist: true })
}

/** Starter markdown for a new record of the given kind. */
export function entityTemplate(kind: EntityKind, name = ''): string {
  const common = { slug: '', kind, name, name_th: '', name_en: '', status: 'draft' as EntityStatus, sources: [] as string[], tags: [] as string[] }
  if (kind === 'monks') {
    return stringifyFrontmatter({ ...common, temple: '', province: '', born: '', died: '' }, `# ${name || '（高僧名）'}

## 概要

## 生涯

## 所属寺院

## 代表的なプラクルアン

## 出典
- 一次資料: 
- Web（未確認）: 
`)
  }
  if (kind === 'temples') {
    return stringifyFrontmatter({ ...common, province: '', founded: '' }, `# ${name || '（寺院名）'}

## 概要

## 歴史

## 関連する高僧

## 所在地

## 出典
- 一次資料: 
- Web（未確認）: 
`)
  }
  return stringifyFrontmatter(common, `# ${name || '（プラクルアン名）'}

## 概要

## 出典
`)
}
