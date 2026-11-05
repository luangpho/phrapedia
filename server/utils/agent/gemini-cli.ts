import type { ChatMessage, ChatPart, ChatStreamEvent } from '../../../shared/types/chat'
import type { SourceSearchHit } from '../../../shared/types/sources'
import type { AgentRunResult } from './anthropic'
import { historyToPlainText } from './history'

export interface GeminiChatOptions {
  /** Prior conversation, excluding the new user message */
  history: ChatMessage[]
  userMessage: string
  draft: string
  system: string
  onEvent: (ev: ChatStreamEvent) => void
  signal?: AbortSignal
}

const DRAFT_OPEN = '```markdown:draft'
const SOURCE_TEXT_LIMIT = 8_000

/** Candidate search terms: names from the draft frontmatter + Thai / katakana / latin / kanji runs in the message. */
export function extractSearchTerms(userMessage: string, draft: string): string[] {
  const terms: string[] = []
  try {
    const { data } = parseFrontmatter<Record<string, unknown>>(draft || '')
    for (const key of ['name', 'name_th', 'name_en']) {
      const v = data[key]
      if (typeof v === 'string' && v.trim()) terms.push(v.trim())
    }
    if (Array.isArray(data.aliases)) {
      for (const a of data.aliases) if (typeof a === 'string' && a.trim()) terms.push(a.trim())
    }
  } catch {
    // invalid frontmatter in the editor — ignore
  }
  const re = /[฀-๿]{2,}|[゠-ヿ]{2,}|[A-Za-z][A-Za-z'-]{2,}|[一-鿿]{2,}/g
  for (const m of userMessage.matchAll(re)) {
    // split katakana names on the middle dot as well (ルアンポー・トー → ルアンポー, トー)
    terms.push(m[0])
    if (m[0].includes('・')) terms.push(...m[0].split('・').filter(s => s.length >= 2))
  }
  const stop = new Set(['について', '作成', '下書き', '一次資料', 'もとに', 'して', 'ください'])
  const uniq: string[] = []
  for (const t of terms.flatMap(t => t.split(/\s+/))) {
    const s = t.replace(/[・]/g, '').length ? t : ''
    if (s && !stop.has(s) && !uniq.includes(s)) uniq.push(s)
  }
  return uniq.slice(0, 8)
}

/** Split the model output into the visible answer and the fenced ```markdown:draft block (if any). */
export function parseDraftBlock(output: string): { text: string; draft?: string } {
  const start = output.indexOf(DRAFT_OPEN)
  if (start < 0) return { text: output.trim() }
  const afterOpen = output.indexOf('\n', start)
  if (afterOpen < 0) return { text: output.slice(0, start).trim() }
  const rest = output.slice(afterOpen + 1)
  // the closing fence is the LAST line consisting only of ``` (the record itself may contain code fences)
  const fenceRe = /^```[ \t]*$/gm
  let close = -1
  let closeEnd = rest.length
  for (const m of rest.matchAll(fenceRe)) {
    close = m.index!
    closeEnd = m.index! + m[0].length
  }
  const draft = (close >= 0 ? rest.slice(0, close) : rest).replace(/\s+$/, '') + '\n'
  const after = close >= 0 ? rest.slice(closeEnd) : ''
  const text = [output.slice(0, start).trim(), after.trim()].filter(Boolean).join('\n\n')
  return { text, draft: draft.trim() ? draft : undefined }
}

async function gatherContext(userMessage: string, draft: string): Promise<{ text: string; used: string[] }> {
  const terms = extractSearchTerms(userMessage, draft)
  if (!terms.length) return { text: '', used: [] }
  let hits: SourceSearchHit[] = []
  try {
    hits = await searchSources(terms.join(' '), 5)
  } catch {
    return { text: '', used: [] }
  }
  if (!hits.length) return { text: '', used: [] }
  const blocks: string[] = ['### 検索結果（上位）']
  for (const h of hits) blocks.push(`- ${h.path}: ${h.snippet || '（ファイル名が一致）'}`)
  const full = hits.filter(h => h.kind === 'markdown' || h.kind === 'text').slice(0, 2)
  for (const h of full) {
    try {
      const src = await readSource(h.path, { maxChars: SOURCE_TEXT_LIMIT })
      if (src.text) blocks.push(`\n### 一次資料: ${h.path}\n\`\`\`\n${src.text}\n\`\`\``)
    } catch {
      // unreadable file — skip
    }
  }
  return { text: blocks.join('\n'), used: hits.map(h => h.path) }
}

/** Single-shot chat via Gemini CLI (no tool calling). The updated record comes back in a ```markdown:draft block. */
export async function runGeminiChat(opts: GeminiChatOptions): Promise<AgentRunResult> {
  const { onEvent, signal } = opts
  const parts: ChatPart[] = []
  try {
    const ctx = await gatherContext(opts.userMessage, opts.draft)
    if (signal?.aborted) return { parts, aborted: true }
    const transcript = historyToPlainText(opts.history, 10)
    const prompt = [
      opts.system,
      '## 一次資料からの抜粋（自動検索）',
      ctx.text || '（該当する一次資料は見つかりませんでした）',
      '## これまでの会話',
      transcript || '（なし）',
      '## 今回のユーザーの依頼',
      opts.userMessage,
      '## 出力の指示',
      [
        '一次資料で足りない部分は Google 検索ツールで Web を調べて補い、Web 由来の事実は「（未確認）」と URL を付けてください。',
        'まず日本語で簡潔な回答（何を調べ、何を変更したか、使った資料の一覧）を書き、',
        'レコードを変更する場合はその後に更新後のレコード全文（frontmatter を含む）を',
        '```markdown:draft で始まり ``` だけの行で終わるコードブロックで 1 つ出力してください。',
      ].join('\n'),
    ].join('\n\n')

    const res = await runGemini({ prompt, noCache: true, signal })
    if (signal?.aborted) return { parts, aborted: true }

    const { text, draft } = parseDraftBlock(res.text)
    if (text) {
      parts.push({ type: 'text', text })
      onEvent({ type: 'text-delta', delta: text })
    }
    if (draft) {
      const summary = 'Gemini CLI による下書きの更新'
      parts.push({ type: 'draft', markdown: draft, summary })
      onEvent({ type: 'draft', markdown: draft, summary })
    }
    if (!text && !draft) {
      const note = '（Gemini CLI から回答がありませんでした）'
      parts.push({ type: 'text', text: note })
      onEvent({ type: 'text-delta', delta: note })
    }
    return { parts }
  } catch (err) {
    if (signal?.aborted) return { parts, aborted: true }
    const e = err as { message?: string; statusMessage?: string }
    const message = `Gemini CLI でエラーが発生しました: ${e?.message || e?.statusMessage || String(err)}`
    onEvent({ type: 'error', message })
    return { parts, error: message }
  }
}
