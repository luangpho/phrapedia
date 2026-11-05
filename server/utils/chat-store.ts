import { randomUUID } from 'node:crypto'
import type { ChatMessage, ChatPart, ChatRole, ChatSession } from '../../shared/types/chat'
import type { EntityKind } from '../../shared/types/entity'

/**
 * Chat history persistence (PGlite tables chat_sessions / chat_messages, see server/utils/db.ts).
 * Sessions are bound to (kind, slug); slug is null while the record is still an unsaved draft.
 */

interface SessionRow {
  id: string
  kind: EntityKind
  slug: string | null
  title: string
  created_at: string | Date
  updated_at: string | Date
}

interface MessageRow {
  id: string
  session_id: string
  role: ChatRole
  parts: ChatPart[] | string
  created_at: string | Date
}

function iso(v: string | Date): string {
  return v instanceof Date ? v.toISOString() : new Date(v).toISOString()
}

function toSession(r: SessionRow): ChatSession {
  return { id: r.id, kind: r.kind, slug: r.slug ?? null, title: r.title, createdAt: iso(r.created_at), updatedAt: iso(r.updated_at) }
}

function toMessage(r: MessageRow): ChatMessage {
  const parts = typeof r.parts === 'string' ? (JSON.parse(r.parts) as ChatPart[]) : r.parts
  return { id: r.id, role: r.role, parts: Array.isArray(parts) ? parts : [], createdAt: iso(r.created_at) }
}

export function newChatId(): string {
  return randomUUID()
}

export async function createChatSession(kind: EntityKind, slug: string | null, title: string): Promise<ChatSession> {
  const db = await useDb()
  const res = await db.query<SessionRow>(
    `INSERT INTO chat_sessions (id, kind, slug, title) VALUES ($1, $2, $3, $4)
     RETURNING id, kind, slug, title, created_at, updated_at`,
    [newChatId(), kind, slug, title.slice(0, 200)],
  )
  return toSession(res.rows[0]!)
}

export async function getChatSession(id: string): Promise<ChatSession | null> {
  const db = await useDb()
  const res = await db.query<SessionRow>(
    'SELECT id, kind, slug, title, created_at, updated_at FROM chat_sessions WHERE id = $1', [id],
  )
  return res.rows[0] ? toSession(res.rows[0]) : null
}

/** Sessions of a record, newest first. `slug === null` lists draft (unsaved) sessions of the kind. */
export async function listChatSessions(kind: EntityKind, slug: string | null): Promise<ChatSession[]> {
  const db = await useDb()
  const res = slug === null
    ? await db.query<SessionRow>(
        `SELECT id, kind, slug, title, created_at, updated_at FROM chat_sessions
         WHERE kind = $1 AND slug IS NULL ORDER BY updated_at DESC LIMIT 100`, [kind])
    : await db.query<SessionRow>(
        `SELECT id, kind, slug, title, created_at, updated_at FROM chat_sessions
         WHERE kind = $1 AND slug = $2 ORDER BY updated_at DESC LIMIT 100`, [kind, slug])
  return res.rows.map(toSession)
}

export async function appendChatMessage(sessionId: string, message: ChatMessage): Promise<ChatMessage> {
  const db = await useDb()
  await db.query(
    `INSERT INTO chat_messages (id, session_id, role, parts, created_at) VALUES ($1, $2, $3, $4, $5)
     ON CONFLICT (id) DO UPDATE SET parts = EXCLUDED.parts`,
    [message.id, sessionId, message.role, JSON.stringify(message.parts), message.createdAt],
  )
  return message
}

export async function listChatMessages(sessionId: string): Promise<ChatMessage[]> {
  const db = await useDb()
  const res = await db.query<MessageRow>(
    `SELECT id, session_id, role, parts, created_at FROM chat_messages
     WHERE session_id = $1 ORDER BY created_at ASC, id ASC`, [sessionId],
  )
  return res.rows.map(toMessage)
}

export async function deleteChatSession(id: string): Promise<boolean> {
  const db = await useDb()
  await db.query('DELETE FROM chat_messages WHERE session_id = $1', [id])
  const res = await db.query('DELETE FROM chat_sessions WHERE id = $1', [id])
  return (res.affectedRows ?? 0) > 0
}

/** Bump updated_at; optionally re-bind the session to a slug (after the draft was saved) or rename it. */
/** Delete specific messages of a session. Returns the number of rows removed. */
export async function deleteChatMessages(sessionId: string, ids: string[]): Promise<number> {
  if (!ids.length) return 0
  const db = await useDb()
  const res = await db.query('DELETE FROM chat_messages WHERE session_id = $1 AND id = ANY($2::text[])', [sessionId, ids])
  return res.affectedRows ?? 0
}

export async function touchChatSession(id: string, patch: { slug?: string | null; title?: string } = {}): Promise<void> {
  const db = await useDb()
  const sets = ['updated_at = now()']
  const params: unknown[] = [id]
  if (patch.slug !== undefined) {
    params.push(patch.slug)
    sets.push(`slug = $${params.length}`)
  }
  if (patch.title !== undefined) {
    params.push(patch.title.slice(0, 200))
    sets.push(`title = $${params.length}`)
  }
  await db.query(`UPDATE chat_sessions SET ${sets.join(', ')} WHERE id = $1`, params)
}
