import { readdir, readFile, stat } from 'node:fs/promises'
import { join, extname, basename, relative } from 'node:path'
import type { SourceContent, SourceFile, SourceFileKind, SourceSearchHit } from '../../shared/types/sources'

const TEXT_EXT = new Set(['.md', '.markdown', '.txt', '.csv', '.json', '.yaml', '.yml'])
const IMAGE_EXT: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
}
const IGNORED_DIRS = new Set(['node_modules', '.git', '.phrapedia', '.nuxt', '.output'])
const MAX_INDEXED_TEXT = 512 * 1024 // chars per file kept in the index

export function sourceKind(ext: string): SourceFileKind {
  const e = ext.toLowerCase()
  if (e === '.md' || e === '.markdown') return 'markdown'
  if (TEXT_EXT.has(e)) return 'text'
  if (e in IMAGE_EXT) return 'image'
  if (e === '.pdf') return 'pdf'
  return 'other'
}

export function imageMime(ext: string): string | undefined {
  return IMAGE_EXT[ext.toLowerCase()]
}

/** Recursively list files under the sources dir (relative POSIX paths). */
export async function listSources(subdir = ''): Promise<SourceFile[]> {
  const base = await sourcesDir()
  const start = safeJoin(base, subdir)
  const out: SourceFile[] = []
  async function walk(dir: string) {
    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code === 'ENOENT') return
      throw err
    }
    for (const e of entries) {
      if (e.name.startsWith('.') || IGNORED_DIRS.has(e.name)) continue
      const full = join(dir, e.name)
      if (e.isDirectory()) {
        await walk(full)
      } else if (e.isFile()) {
        const st = await stat(full)
        const ext = extname(e.name)
        out.push({
          path: toPosix(relative(base, full)),
          name: basename(e.name),
          ext,
          kind: sourceKind(ext),
          size: st.size,
          mtime: st.mtime.toISOString(),
        })
      }
    }
  }
  await walk(start)
  return out.sort((a, b) => a.path.localeCompare(b.path))
}

/** Read one source file. Text for markdown/text, base64 for images. */
export async function readSource(relPath: string, opts: { maxChars?: number } = {}): Promise<SourceContent> {
  const base = await sourcesDir()
  const full = safeJoin(base, relPath)
  let st
  try {
    st = await stat(full)
  } catch {
    throw createError({ statusCode: 404, statusMessage: 'Not found', message: `Source not found: ${relPath}` })
  }
  if (!st.isFile()) throw createError({ statusCode: 400, statusMessage: 'Not a file', message: relPath })
  const ext = extname(full)
  const kind = sourceKind(ext)
  if (kind === 'markdown' || kind === 'text') {
    let text = await readFile(full, 'utf8')
    if (opts.maxChars && text.length > opts.maxChars) text = text.slice(0, opts.maxChars) + `\n\n…(truncated, ${text.length} chars total)`
    return { path: relPath, kind, text, size: st.size }
  }
  if (kind === 'image') {
    const buf = await readFile(full)
    return { path: relPath, kind, base64: buf.toString('base64'), mimeType: imageMime(ext), size: st.size }
  }
  return { path: relPath, kind, size: st.size, mimeType: kind === 'pdf' ? 'application/pdf' : undefined }
}

/** Absolute path for streaming binary files (images) to the browser. */
export async function sourceAbsolutePath(relPath: string): Promise<{ full: string; mimeType?: string; kind: SourceFileKind }> {
  const base = await sourcesDir()
  const full = safeJoin(base, relPath)
  const ext = extname(full)
  return { full, mimeType: imageMime(ext) ?? (ext.toLowerCase() === '.pdf' ? 'application/pdf' : undefined), kind: sourceKind(ext) }
}

/**
 * (Re)build the PGlite index of text sources. Incremental: unchanged files (same mtime+size) are skipped,
 * deleted files are removed. Returns counts.
 */
export async function reindexSources(): Promise<{ indexed: number; removed: number; total: number }> {
  const db = await useDb()
  const base = await sourcesDir()
  const files = await listSources()
  const existing = await db.query<{ path: string; size: string; mtime: string }>('SELECT path, size, mtime FROM source_index')
  const known = new Map(existing.rows.map(r => [r.path, r]))
  let indexed = 0
  const seen = new Set<string>()
  for (const f of files) {
    seen.add(f.path)
    const prev = known.get(f.path)
    if (prev && Number(prev.size) === f.size && new Date(prev.mtime).getTime() === new Date(f.mtime).getTime()) continue
    let content: string | null = null
    if (f.kind === 'markdown' || f.kind === 'text') {
      content = (await readFile(join(base, f.path), 'utf8')).slice(0, MAX_INDEXED_TEXT)
    }
    await db.query(
      `INSERT INTO source_index (path, kind, size, mtime, content, indexed_at) VALUES ($1, $2, $3, $4, $5, now())
       ON CONFLICT (path) DO UPDATE SET kind = EXCLUDED.kind, size = EXCLUDED.size, mtime = EXCLUDED.mtime, content = EXCLUDED.content, indexed_at = now()`,
      [f.path, f.kind, f.size, f.mtime, content],
    )
    indexed++
  }
  let removed = 0
  for (const p of known.keys()) {
    if (!seen.has(p)) {
      await db.query('DELETE FROM source_index WHERE path = $1', [p])
      removed++
    }
  }
  return { indexed, removed, total: files.length }
}

/**
 * Search primary sources. Thai and Japanese have no word boundaries, so matching is substring-based
 * (case-insensitive ILIKE) over every whitespace-separated term; file names are matched too.
 * Score = number of terms matched in content ×2 + matches in path.
 */
export async function searchSources(query: string, limit = 20): Promise<SourceSearchHit[]> {
  const terms = query.split(/\s+/).map(t => t.trim()).filter(Boolean).slice(0, 8)
  if (!terms.length) return []
  await reindexSources()
  const db = await useDb()
  const res = await db.query<{ path: string; kind: SourceFileKind; content: string | null }>(
    'SELECT path, kind, content FROM source_index',
  )
  const hits: SourceSearchHit[] = []
  for (const row of res.rows) {
    const content = row.content ?? ''
    const lowerContent = content.toLowerCase()
    const lowerPath = row.path.toLowerCase()
    let score = 0
    let firstIdx = -1
    for (const t of terms) {
      const lt = t.toLowerCase()
      const idx = lowerContent.indexOf(lt)
      if (idx >= 0) {
        score += 2
        if (firstIdx < 0 || idx < firstIdx) firstIdx = idx
      }
      if (lowerPath.includes(lt)) score += 1
    }
    if (score === 0) continue
    const snippet = firstIdx >= 0 ? excerpt(content, firstIdx, 160) : ''
    hits.push({ path: row.path, kind: row.kind, snippet, score })
  }
  return hits.sort((a, b) => b.score - a.score || a.path.localeCompare(b.path)).slice(0, limit)
}

function excerpt(text: string, at: number, radius: number): string {
  const start = Math.max(0, at - radius)
  const end = Math.min(text.length, at + radius)
  return (start > 0 ? '…' : '') + text.slice(start, end).replace(/\s+/g, ' ') + (end < text.length ? '…' : '')
}
