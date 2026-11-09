import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { PGlite } from '@electric-sql/pglite'

/**
 * PGlite (embedded Postgres) is used for app state that is NOT part of the shared dataset:
 * settings, chat sessions/messages, the primary-source search index and the web-research cache.
 * Records themselves live as markdown files (data/<kind>/<slug>.md) so that git stays the source of truth.
 */
const MIGRATIONS: { id: number; sql: string }[] = [
  {
    id: 1,
    sql: `
      CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value JSONB NOT NULL,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
      );
      CREATE TABLE IF NOT EXISTS chat_sessions (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        slug TEXT,
        title TEXT NOT NULL DEFAULT '',
        created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
      );
      CREATE INDEX IF NOT EXISTS chat_sessions_kind_slug ON chat_sessions (kind, slug);
      CREATE TABLE IF NOT EXISTS chat_messages (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
        role TEXT NOT NULL,
        parts JSONB NOT NULL,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now()
      );
      CREATE INDEX IF NOT EXISTS chat_messages_session ON chat_messages (session_id, created_at);
      CREATE TABLE IF NOT EXISTS source_index (
        path TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        size BIGINT NOT NULL,
        mtime TIMESTAMPTZ NOT NULL,
        content TEXT,
        indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
      );
      CREATE TABLE IF NOT EXISTS research_cache (
        query_hash TEXT PRIMARY KEY,
        query TEXT NOT NULL,
        result TEXT NOT NULL,
        model TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now()
      );
      CREATE TABLE IF NOT EXISTS schema_migrations (
        id INTEGER PRIMARY KEY,
        applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
      );
    `,
  },
  {
    id: 2,
    sql: `ALTER TABLE chat_sessions ADD COLUMN IF NOT EXISTS claude_session_id TEXT;`,
  },
]

type DbGlobal = typeof globalThis & { __phrapediaDb?: Promise<PGlite> }

async function openDb(): Promise<PGlite> {
  const dir = join(stateDir(), 'pglite')
  await mkdir(dir, { recursive: true })
  const db = await PGlite.create(dir)
  await db.exec(`CREATE TABLE IF NOT EXISTS schema_migrations (id INTEGER PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now());`)
  const applied = await db.query<{ id: number }>('SELECT id FROM schema_migrations')
  const done = new Set(applied.rows.map(r => Number(r.id)))
  for (const m of MIGRATIONS) {
    if (done.has(m.id)) continue
    await db.exec(m.sql)
    await db.query('INSERT INTO schema_migrations (id) VALUES ($1)', [m.id])
  }
  return db
}

/**
 * Lazily created singleton PGlite instance. Memoised as a Promise so concurrent first requests share one
 * `PGlite.create()`; cleared on failure so a failed open can be retried. PGlite has no data-directory lock,
 * so the instance is closed on Nitro's `close` hook (see server/plugins/pglite-close.ts).
 */
export function useDb(): Promise<PGlite> {
  const g = globalThis as DbGlobal
  if (!g.__phrapediaDb) {
    g.__phrapediaDb = openDb().catch((err) => {
      g.__phrapediaDb = undefined
      throw err
    })
  }
  return g.__phrapediaDb
}
