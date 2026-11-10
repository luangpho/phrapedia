import type { EntityKind } from './entity'

export type ChatRole = 'user' | 'assistant' | 'system'

export type ChatPart =
  | { type: 'text'; text: string }
  | {
      type: 'tool'
      toolCallId: string
      name: string
      input: unknown
      output?: unknown
      isError?: boolean
      state: 'running' | 'done'
    }
  | { type: 'draft'; markdown: string; summary?: string }

export interface ChatMessage {
  id: string
  role: ChatRole
  parts: ChatPart[]
  createdAt: string
}

export interface ChatSession {
  id: string
  kind: EntityKind
  /** null while the record has not been saved yet (draft) */
  slug: string | null
  title: string
  /** Claude Code CLI session id used for --resume (null until the first turn ran, or after history edits) */
  claudeSessionId?: string | null
  createdAt: string
  updatedAt: string
}

/** Request of the `chat_send` command */
export interface ChatRequest {
  /** Client-generated UUID; the key of `chat_abort` (the first turn has no sessionId yet) */
  turnId: string
  sessionId?: string
  kind: EntityKind
  slug?: string | null
  /** The user's message */
  message: string
  /** Current editor content (full markdown with frontmatter). The AI uses it as the working draft. */
  draft: string
}

/** Events streamed back by `chat_send` through the Tauri Channel */
export type ChatStreamEvent =
  | { type: 'session'; sessionId: string; userMessageId: string; assistantMessageId: string }
  | { type: 'text-delta'; delta: string }
  | { type: 'tool-start'; toolCallId: string; name: string; input: unknown }
  | { type: 'tool-result'; toolCallId: string; name: string; output: unknown; isError?: boolean }
  | { type: 'draft'; markdown: string; summary?: string }
  /** The AI saved a record through save_entity (the UI should refresh / navigate) */
  | { type: 'saved'; kind: EntityKind; slug: string; path?: string }
  | { type: 'done'; message: ChatMessage }
  | { type: 'error'; message: string }
