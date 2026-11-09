import type { ChatPart, ChatStreamEvent } from '../../../shared/types/chat'
import type { EntityKind } from '../../../shared/types/entity'

/** Name under which the app registers its own MCP endpoint in --mcp-config. Tools show up as mcp__phrapedia__<name>. */
export const MCP_SERVER_NAME = 'phrapedia'
export const MCP_TOOL_PREFIX = `mcp__${MCP_SERVER_NAME}__`
export const UPDATE_DRAFT_TOOL = 'update_draft'

/** Same limit as the former SDK loop: tool output sent to the UI / stored in history is cut here. */
export const EVENT_OUTPUT_LIMIT = 20_000

function truncate(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit)}\n…(${text.length - limit} 文字省略)` : text
}

/** `mcp__phrapedia__search_sources` -> `search_sources` (the UI and the stored history use the short name). */
export function shortToolName(name: string): string {
  return name.startsWith(MCP_TOOL_PREFIX) ? name.slice(MCP_TOOL_PREFIX.length) : name
}

/** save_entity returns JSON like { ok, kind, slug, path }; extract what the UI needs. */
export function parseSavedResult(text: string): { kind: EntityKind; slug: string; path?: string } | null {
  try {
    const data = JSON.parse(text) as { ok?: boolean; kind?: string; slug?: string; path?: string }
    if (data?.ok && typeof data.slug === 'string' && (data.kind === 'monks' || data.kind === 'temples' || data.kind === 'amulets')) {
      return { kind: data.kind, slug: data.slug, path: data.path }
    }
  } catch {
    // not JSON
  }
  return null
}

export interface ClaudeRunResult {
  /** Assistant parts in order (text, tool, draft) */
  parts: ChatPart[]
  /** Session id reported by the CLI (system/init or result) */
  sessionId?: string
  /** The CLI's `result` message was seen */
  finished: boolean
  /** Error text from an is_error result (not yet turned into a user-facing message) */
  resultError?: string
  /** subtype of the result message (success | error_during_execution | error_max_turns ...) */
  resultSubtype?: string
}

type Json = Record<string, any>

/**
 * Converts the line-delimited JSON of `claude -p --output-format stream-json --verbose --include-partial-messages`
 * into ChatStreamEvents. Feed it one stdout line at a time; it keeps the assistant parts for storage.
 *
 * Events used (verified against Claude Code 2.1.317):
 *  - `system/init`                      -> session id
 *  - `stream_event` content_block_delta -> text-delta (partial messages)
 *  - `assistant` message                -> tool_use blocks -> tool-start (update_draft -> draft); text blocks of messages
 *                                          that were not streamed as deltas are emitted as text-delta
 *  - `user` message                     -> tool_result blocks -> tool-result (+ saved for save_entity)
 *  - `result`                           -> final status / error
 */
export class ClaudeStreamParser {
  readonly parts: ChatPart[] = []
  sessionId?: string
  finished = false
  resultError?: string
  resultSubtype?: string

  private streamedMessages = new Set<string>()
  private currentMessageId?: string
  private seenToolUses = new Set<string>()
  /** tool_use id -> short tool name */
  private toolNames = new Map<string, string>()
  private toolParts = new Map<string, Extract<ChatPart, { type: 'tool' }>>()
  private hiddenToolUses = new Set<string>()

  constructor(private readonly onEvent: (ev: ChatStreamEvent) => void) {}

  result(): ClaudeRunResult {
    return { parts: this.parts, sessionId: this.sessionId, finished: this.finished, resultError: this.resultError, resultSubtype: this.resultSubtype }
  }

  /** Mark tool parts that never got a result (killed / aborted run). */
  markInterrupted(): void {
    for (const p of this.parts) {
      if (p.type === 'tool' && p.state === 'running') {
        p.state = 'done'
        p.isError = true
        p.output = p.output ?? '（中断されました）'
      }
    }
  }

  feed(line: string): void {
    const trimmed = line.trim()
    if (!trimmed) return
    let msg: Json
    try {
      msg = JSON.parse(trimmed) as Json
    } catch {
      return // non-JSON noise on stdout
    }
    if (!msg || typeof msg !== 'object') return
    // Sub-agent traffic is not part of the conversation shown to the user.
    if (msg.parent_tool_use_id) return

    switch (msg.type) {
      case 'system':
        if (msg.subtype === 'init' && typeof msg.session_id === 'string') this.sessionId = msg.session_id
        break
      case 'stream_event':
        this.onStreamEvent(msg.event as Json | undefined)
        break
      case 'assistant':
        this.onAssistant(msg.message as Json | undefined)
        break
      case 'user':
        this.onUser(msg.message as Json | undefined)
        break
      case 'result':
        this.onResult(msg)
        break
    }
  }

  private appendText(delta: string): void {
    if (!delta) return
    const last = this.parts[this.parts.length - 1]
    if (last?.type === 'text') last.text += delta
    else this.parts.push({ type: 'text', text: delta })
    this.onEvent({ type: 'text-delta', delta })
  }

  private onStreamEvent(ev: Json | undefined): void {
    if (!ev) return
    if (ev.type === 'message_start') {
      this.currentMessageId = typeof ev.message?.id === 'string' ? ev.message.id : undefined
      return
    }
    if (ev.type === 'content_block_delta' && ev.delta?.type === 'text_delta' && typeof ev.delta.text === 'string') {
      if (this.currentMessageId) this.streamedMessages.add(this.currentMessageId)
      this.appendText(ev.delta.text)
    }
  }

  private onAssistant(message: Json | undefined): void {
    if (!message || !Array.isArray(message.content)) return
    const streamed = typeof message.id === 'string' && this.streamedMessages.has(message.id)
    for (const block of message.content as Json[]) {
      if (block.type === 'text' && !streamed && typeof block.text === 'string') {
        // not streamed as deltas (partial messages off / synthetic message): emit it whole
        this.appendText(block.text)
      } else if (block.type === 'tool_use') {
        this.onToolUse(block)
      }
    }
  }

  private onToolUse(block: Json): void {
    const id = String(block.id ?? '')
    if (!id || this.seenToolUses.has(id)) return
    this.seenToolUses.add(id)
    const name = shortToolName(String(block.name ?? ''))
    const input = (block.input && typeof block.input === 'object' ? block.input : {}) as Record<string, unknown>
    this.toolNames.set(id, name)

    if (name === UPDATE_DRAFT_TOOL) {
      this.hiddenToolUses.add(id)
      const markdown = typeof input.markdown === 'string' ? input.markdown : ''
      const summary = typeof input.summary === 'string' && input.summary.trim() ? input.summary.trim() : undefined
      if (markdown.trim()) {
        this.parts.push({ type: 'draft', markdown, summary })
        this.onEvent({ type: 'draft', markdown, summary })
      }
      return
    }

    const part: Extract<ChatPart, { type: 'tool' }> = { type: 'tool', toolCallId: id, name, input, state: 'running' }
    this.parts.push(part)
    this.toolParts.set(id, part)
    this.onEvent({ type: 'tool-start', toolCallId: id, name, input })
  }

  private onUser(message: Json | undefined): void {
    if (!message || !Array.isArray(message.content)) return
    for (const block of message.content as Json[]) {
      if (block.type !== 'tool_result') continue
      const id = String(block.tool_use_id ?? '')
      if (!id || this.hiddenToolUses.has(id)) continue
      const name = this.toolNames.get(id) ?? 'unknown'
      const { text, images } = flattenToolResult(block.content)
      const isError = Boolean(block.is_error)
      const output = truncate(text, EVENT_OUTPUT_LIMIT) + (images ? `\n[画像 ${images} 件]` : '')
      const part = this.toolParts.get(id)
      if (part) {
        part.output = output
        part.isError = isError || undefined
        part.state = 'done'
      }
      this.onEvent({ type: 'tool-result', toolCallId: id, name, output, isError: isError || undefined })
      if (name === 'save_entity' && !isError) {
        const saved = parseSavedResult(text)
        if (saved) this.onEvent({ type: 'saved', ...saved })
      }
    }
  }

  private onResult(msg: Json): void {
    this.finished = true
    if (typeof msg.session_id === 'string') this.sessionId = msg.session_id
    this.resultSubtype = typeof msg.subtype === 'string' ? msg.subtype : undefined
    if (msg.is_error || (this.resultSubtype && this.resultSubtype !== 'success')) {
      const text = typeof msg.result === 'string' && msg.result.trim()
        ? msg.result.trim()
        : Array.isArray(msg.errors) ? msg.errors.map(String).join('; ') : (this.resultSubtype ?? 'error')
      this.resultError = text
    }
  }
}

/** tool_result content is a string or an array of {type:'text'|'image'} blocks. */
export function flattenToolResult(content: unknown): { text: string; images: number } {
  if (typeof content === 'string') return { text: content, images: 0 }
  if (!Array.isArray(content)) return { text: content == null ? '' : JSON.stringify(content), images: 0 }
  const texts: string[] = []
  let images = 0
  for (const c of content as Json[]) {
    if (c?.type === 'text' && typeof c.text === 'string') texts.push(c.text)
    else if (c?.type === 'image') images++
    else if (c?.type) texts.push(`[${c.type} content]`)
  }
  return { text: texts.join('\n'), images }
}
