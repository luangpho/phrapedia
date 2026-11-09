import type { ChatMessage, ChatPart } from '../../../shared/types/chat'

const PART_LIMIT = 1_500
const TOTAL_LIMIT = 24_000

function clip(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit)}…(省略)` : text
}

function partText(part: ChatPart): string {
  if (part.type === 'text') return part.text.trim()
  if (part.type === 'draft') return `（update_draft で下書きを更新しました${part.summary ? `: ${part.summary}` : ''}）`
  return `（ツール ${part.name} を実行）`
}

/**
 * Plain-text digest of the stored conversation. Used when the Claude Code CLI session cannot be resumed
 * (other machine, session file gone): the digest is put in front of the prompt so the new session keeps the context.
 * Older messages are dropped first when the digest gets too long.
 */
export function summarizeHistory(history: ChatMessage[]): string {
  const lines: string[] = []
  for (const msg of history) {
    if (msg.role === 'system') continue
    const text = msg.parts.map(partText).filter(Boolean).join('\n').trim()
    if (!text) continue
    lines.push(`${msg.role === 'user' ? 'ユーザー' : 'アシスタント'}: ${clip(text, PART_LIMIT)}`)
  }
  let total = 0
  const kept: string[] = []
  for (let i = lines.length - 1; i >= 0; i--) {
    total += lines[i]!.length + 1
    if (total > TOTAL_LIMIT) break
    kept.unshift(lines[i]!)
  }
  return kept.join('\n')
}

/** Prompt for a fresh CLI session that continues an existing conversation. */
export function withHistoryDigest(prompt: string, history: ChatMessage[]): string {
  const digest = summarizeHistory(history)
  if (!digest) return prompt
  return [
    '以下は、この会話のこれまでのやり取りの記録です（別のセッションから引き継いだ文脈。内容は既に実行済みで、再実行は不要です）。',
    '<conversation_so_far>',
    digest,
    '</conversation_so_far>',
    '',
    '上記を踏まえて、次のユーザーの発言に答えてください。',
    '',
    prompt,
  ].join('\n')
}
