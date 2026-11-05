import type Anthropic from '@anthropic-ai/sdk'
import type { ChatMessage, ChatPart } from '../../../shared/types/chat'

/** Text of a tool part's output as stored in the history (truncated in the event already). */
export function toolOutputText(output: unknown): string {
  if (output === undefined || output === null) return ''
  if (typeof output === 'string') return output
  try {
    return JSON.stringify(output)
  } catch {
    return String(output)
  }
}

function draftNote(part: Extract<ChatPart, { type: 'draft' }>): string {
  return `（update_draft で下書きを更新しました${part.summary ? `: ${part.summary}` : ''}）`
}

/**
 * Convert stored ChatMessages into Anthropic MessageParams that form a valid history:
 *  - text parts → text blocks
 *  - tool parts → assistant tool_use block, followed by a user message holding the matching tool_result
 *  - draft parts → a short text note (the full markdown is already in the system prompt as the current draft)
 * Consecutive same-role messages are allowed by the API (they are merged into one turn).
 */
export function toAnthropicMessages(history: ChatMessage[]): Anthropic.MessageParam[] {
  const out: Anthropic.MessageParam[] = []

  for (const msg of history) {
    if (msg.role === 'system') continue
    if (msg.role === 'user') {
      const text = msg.parts
        .map(p => (p.type === 'text' ? p.text : p.type === 'draft' ? draftNote(p) : ''))
        .filter(Boolean)
        .join('\n')
        .trim()
      if (text) out.push({ role: 'user', content: text })
      continue
    }

    // assistant
    let blocks: Anthropic.ContentBlockParam[] = []
    let results: Anthropic.ToolResultBlockParam[] = []
    const flush = () => {
      if (blocks.length) out.push({ role: 'assistant', content: blocks })
      if (results.length) out.push({ role: 'user', content: results })
      blocks = []
      results = []
    }
    for (const part of msg.parts) {
      if (part.type === 'tool') {
        blocks.push({
          type: 'tool_use',
          id: part.toolCallId,
          name: part.name,
          input: (part.input && typeof part.input === 'object' ? part.input : {}) as Record<string, unknown>,
        })
        const text = part.state === 'done' ? toolOutputText(part.output) : '（中断されました）'
        results.push({
          type: 'tool_result',
          tool_use_id: part.toolCallId,
          content: text || '(empty)',
          ...(part.isError || part.state !== 'done' ? { is_error: true } : {}),
        })
        continue
      }
      // a text/draft part after tool calls starts a new assistant message (after the tool results)
      if (results.length) flush()
      const text = part.type === 'text' ? part.text : draftNote(part)
      if (text.trim()) blocks.push({ type: 'text', text })
    }
    flush()
  }

  // The API requires the conversation to start with a user turn.
  while (out.length && out[0]!.role !== 'user') out.shift()
  return out
}

/** Plain-text transcript (for providers without structured history, e.g. Gemini CLI). */
export function historyToPlainText(history: ChatMessage[], maxTurns = 10): string {
  const msgs = history.filter(m => m.role !== 'system').slice(-maxTurns * 2)
  return msgs.map((m) => {
    const who = m.role === 'user' ? 'ユーザー' : 'アシスタント'
    const body = m.parts.map((p) => {
      if (p.type === 'text') return p.text
      if (p.type === 'draft') return draftNote(p)
      return `（ツール ${p.name} を使用）`
    }).filter(Boolean).join('\n').trim()
    return body ? `### ${who}\n${body}` : ''
  }).filter(Boolean).join('\n\n')
}
