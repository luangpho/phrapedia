import Anthropic from '@anthropic-ai/sdk'
import type { ChatMessage, ChatPart, ChatStreamEvent } from '../../../shared/types/chat'
import type { EntityKind } from '../../../shared/types/entity'
import type { McpBridge } from '../mcp-bridge'
import { toolResultText } from '../mcp-bridge'
import { toAnthropicMessages } from './history'

export const UPDATE_DRAFT_TOOL: Anthropic.Tool = {
  name: 'update_draft',
  description: [
    'Replace the working draft shown in the user\'s editor with new content. Pass the FULL updated markdown',
    '(YAML frontmatter + body), never a diff or a fragment. This does NOT save the file; the user saves it.',
    'Use this whenever you add or change record content.',
  ].join(' '),
  input_schema: {
    type: 'object',
    properties: {
      markdown: { type: 'string', description: 'Full markdown of the record including the YAML frontmatter' },
      summary: { type: 'string', description: 'One-line summary (Japanese) of what changed' },
    },
    required: ['markdown'],
  },
}

const MAX_ITERATIONS = 12
const EVENT_OUTPUT_LIMIT = 20_000
const MODEL_TOOL_RESULT_LIMIT = 200_000
const IMAGE_TYPES = new Set(['image/jpeg', 'image/png', 'image/gif', 'image/webp'])

export interface AnthropicAgentOptions {
  /** Prior conversation (including the new user message as the last entry) */
  messages: ChatMessage[]
  system: string
  bridge: McpBridge
  model: string
  maxTokens: number
  apiKey: string
  onEvent: (ev: ChatStreamEvent) => void
  signal?: AbortSignal
}

export interface AgentRunResult {
  /** Assistant parts in order (text, tool, draft) */
  parts: ChatPart[]
  /** Readable error message when the run failed (an {type:'error'} event was already emitted) */
  error?: string
  aborted?: boolean
}

/** Map MCP tool descriptors to Anthropic tool definitions and add the virtual update_draft tool. */
export function anthropicTools(bridge: McpBridge): Anthropic.Tool[] {
  const tools: Anthropic.Tool[] = bridge.tools.map(t => ({
    name: t.name,
    description: t.description ?? t.title ?? t.name,
    input_schema: { ...(t.inputSchema as Record<string, unknown>), type: 'object' } as Anthropic.Tool.InputSchema,
  }))
  tools.push(UPDATE_DRAFT_TOOL)
  return tools
}

/** Human-readable (Japanese) error message for Anthropic SDK errors. */
export function describeAnthropicError(err: unknown, model: string): string {
  if (err instanceof Anthropic.AuthenticationError) return 'Anthropic の API キーが無効です（401）。.env の NUXT_ANTHROPIC_API_KEY を確認してください。'
  if (err instanceof Anthropic.PermissionDeniedError) return 'この API キーでは利用できません（403）。'
  if (err instanceof Anthropic.NotFoundError) return `モデル「${model}」が見つかりません（404）。設定のモデル名を確認してください。`
  if (err instanceof Anthropic.RateLimitError) return 'Anthropic API のレート制限に達しました（429）。しばらく待ってから再試行してください。'
  if (err instanceof Anthropic.BadRequestError) return `リクエストが不正です（400）: ${err.message}`
  if (err instanceof Anthropic.InternalServerError) return `Anthropic API が一時的に利用できません（${err.status}）。しばらくして再試行してください。`
  if (err instanceof Anthropic.APIConnectionError) return 'Anthropic API に接続できませんでした。ネットワークを確認してください。'
  if (err instanceof Anthropic.APIError) return `Anthropic API エラー（${err.status ?? '?'}）: ${err.message}`
  return (err as Error)?.message || String(err)
}

function truncate(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit)}\n…(${text.length - limit} 文字省略)` : text
}

/**
 * Agentic tool-calling loop with streaming (Anthropic Messages API).
 * Text deltas, tool calls and draft updates are reported through onEvent as they happen.
 */
export async function runAnthropicAgent(opts: AnthropicAgentOptions): Promise<AgentRunResult> {
  const { bridge, onEvent, signal, model } = opts
  const client = new Anthropic({ apiKey: opts.apiKey })
  const tools = anthropicTools(bridge)
  const messages: Anthropic.MessageParam[] = toAnthropicMessages(opts.messages)
  const parts: ChatPart[] = []

  const appendText = (delta: string) => {
    const last = parts[parts.length - 1]
    if (last?.type === 'text') last.text += delta
    else parts.push({ type: 'text', text: delta })
  }

  try {
    for (let iteration = 0; iteration < MAX_ITERATIONS; iteration++) {
      if (signal?.aborted) return { parts, aborted: true }

      const stream = client.messages.stream(
        { model, max_tokens: opts.maxTokens, system: opts.system, tools, messages },
        { signal: signal ?? undefined },
      )
      stream.on('text', (delta) => {
        appendText(delta)
        onEvent({ type: 'text-delta', delta })
      })
      const final = await stream.finalMessage()

      if (final.stop_reason === 'refusal') {
        const note = '\n\n（モデルがこの依頼への回答を辞退しました。表現を変えて再度お試しください。）'
        appendText(note)
        onEvent({ type: 'text-delta', delta: note })
        break
      }

      const toolUses = final.content.filter((b): b is Anthropic.ToolUseBlock => b.type === 'tool_use')
      if (final.stop_reason !== 'tool_use' || !toolUses.length) {
        if (final.stop_reason === 'max_tokens') {
          const note = '\n\n（出力が上限トークン数に達したため途中で終了しました。設定の最大トークン数を増やせます。）'
          appendText(note)
          onEvent({ type: 'text-delta', delta: note })
        }
        break
      }

      // Keep the assistant turn verbatim (thinking + text + tool_use blocks) so the next request is valid.
      messages.push({ role: 'assistant', content: final.content })

      const results: Anthropic.ToolResultBlockParam[] = []
      for (const use of toolUses) {
        const input = (use.input && typeof use.input === 'object' ? use.input : {}) as Record<string, unknown>

        if (use.name === UPDATE_DRAFT_TOOL.name) {
          const markdown = typeof input.markdown === 'string' ? input.markdown : ''
          const summary = typeof input.summary === 'string' && input.summary.trim() ? input.summary.trim() : undefined
          if (!markdown.trim()) {
            results.push({ type: 'tool_result', tool_use_id: use.id, is_error: true, content: 'markdown is required (full record with frontmatter)' })
            continue
          }
          parts.push({ type: 'draft', markdown, summary })
          onEvent({ type: 'draft', markdown, summary })
          results.push({ type: 'tool_result', tool_use_id: use.id, content: 'draft updated (the editor now shows this content; it is not saved yet)' })
          continue
        }

        const part: Extract<ChatPart, { type: 'tool' }> = { type: 'tool', toolCallId: use.id, name: use.name, input, state: 'running' }
        parts.push(part)
        onEvent({ type: 'tool-start', toolCallId: use.id, name: use.name, input })

        let text: string
        let images: { data: string; mimeType: string }[] = []
        let isError = false
        try {
          const res = toolResultText(await bridge.callTool(use.name, input))
          text = res.text
          images = res.images
          isError = res.isError
        } catch (err) {
          text = `Tool error: ${(err as Error)?.message || String(err)}`
          isError = true
        }

        const eventOutput = truncate(text, EVENT_OUTPUT_LIMIT) + (images.length ? `\n[画像 ${images.length} 件]` : '')
        part.output = eventOutput
        part.isError = isError || undefined
        part.state = 'done'
        onEvent({ type: 'tool-result', toolCallId: use.id, name: use.name, output: eventOutput, isError: isError || undefined })
        if (use.name === 'save_entity' && !isError) {
          const saved = parseSavedResult(text)
          if (saved) onEvent({ type: 'saved', ...saved })
        }

        const content: Array<Anthropic.TextBlockParam | Anthropic.ImageBlockParam> = [
          { type: 'text', text: truncate(text, MODEL_TOOL_RESULT_LIMIT) || '(empty)' },
        ]
        for (const img of images) {
          if (!IMAGE_TYPES.has(img.mimeType)) continue
          content.push({
            type: 'image',
            source: { type: 'base64', media_type: img.mimeType as Anthropic.Base64ImageSource['media_type'], data: img.data },
          })
        }
        results.push({ type: 'tool_result', tool_use_id: use.id, content, ...(isError ? { is_error: true } : {}) })
      }
      // All results of one assistant turn go back in a single user message.
      messages.push({ role: 'user', content: results })

      if (iteration === MAX_ITERATIONS - 1) {
        const note = '\n\n（ツール呼び出しの上限回数に達したため中断しました。続けるには「続けて」と送ってください。）'
        appendText(note)
        onEvent({ type: 'text-delta', delta: note })
      }
    }
    return { parts }
  } catch (err) {
    if (err instanceof Anthropic.APIUserAbortError || signal?.aborted) {
      markInterrupted(parts)
      return { parts, aborted: true }
    }
    markInterrupted(parts)
    const message = describeAnthropicError(err, model)
    onEvent({ type: 'error', message })
    return { parts, error: message }
  }
}

/** save_entity returns JSON like { ok, kind, slug, path }; extract what the UI needs. */
function parseSavedResult(text: string): { kind: EntityKind; slug: string; path?: string } | null {
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

function markInterrupted(parts: ChatPart[]) {
  for (const p of parts) {
    if (p.type === 'tool' && p.state === 'running') {
      p.state = 'done'
      p.isError = true
      p.output = p.output ?? '（中断されました）'
    }
  }
}

