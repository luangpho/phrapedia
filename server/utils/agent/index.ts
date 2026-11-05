import type { ChatMessage, ChatRequest, ChatSession, ChatStreamEvent } from '../../../shared/types/chat'
import { runAnthropicAgent } from './anthropic'
import type { AgentRunResult } from './anthropic'
import { runGeminiChat } from './gemini-cli'
import { buildSystemPrompt } from './prompt'

export const NO_ANTHROPIC_KEY_MESSAGE
  = 'Anthropic API キーが設定されていません（.env の NUXT_ANTHROPIC_API_KEY）。設定で Gemini CLI プロバイダに切り替えることもできます。'

export interface ChatTurnResult {
  session: ChatSession
  userMessage: ChatMessage
  assistantMessage: ChatMessage
  /** Set when the provider failed; an {type:'error'} event has already been emitted */
  error?: string
  aborted?: boolean
}

export function anthropicApiKey(): string {
  const cfg = useRuntimeConfig()
  return String(cfg.anthropicApiKey || process.env.ANTHROPIC_API_KEY || '').trim()
}

/** Prepare the session (create / load) and the message ids before any provider work starts. */
export async function prepareChatTurn(request: ChatRequest): Promise<{ session: ChatSession; userMessage: ChatMessage; assistantId: string }> {
  const settings = await getSettings()
  if (settings.ai.provider === 'anthropic' && !anthropicApiKey()) {
    throw createError({ statusCode: 400, statusMessage: 'Anthropic API key missing', message: NO_ANTHROPIC_KEY_MESSAGE })
  }
  const slug = request.slug ?? null
  let session: ChatSession | null = request.sessionId ? await getChatSession(request.sessionId) : null
  if (session && session.kind !== request.kind) session = null
  if (!session) {
    const title = request.message.trim().replace(/\s+/g, ' ').slice(0, 60)
    session = await createChatSession(request.kind, slug, title || '新しい会話')
  } else if (slug && session.slug !== slug) {
    // the draft was saved in the meantime: bind the conversation to the record
    await touchChatSession(session.id, { slug })
    session = { ...session, slug }
  }
  const userMessage: ChatMessage = {
    id: newChatId(),
    role: 'user',
    parts: [{ type: 'text', text: request.message }],
    createdAt: new Date().toISOString(),
  }
  return { session, userMessage, assistantId: newChatId() }
}

/**
 * Run one chat turn: store the user message, run the configured provider (Anthropic tool loop, or Gemini CLI
 * fallback), store the assistant message and touch the session.
 */
export async function runChatTurn(
  request: ChatRequest,
  onEvent: (ev: ChatStreamEvent) => void,
  signal?: AbortSignal,
  prepared?: Awaited<ReturnType<typeof prepareChatTurn>>,
): Promise<ChatTurnResult> {
  const settings = await getSettings()
  const { session, userMessage, assistantId } = prepared ?? await prepareChatTurn(request)
  const history = await listChatMessages(session.id)
  await appendChatMessage(session.id, userMessage)

  let result: AgentRunResult
  if (settings.ai.provider === 'gemini-cli') {
    const system = buildSystemPrompt({ kind: request.kind, slug: session.slug, draft: request.draft, settings, toolNames: [], noTools: true })
    result = await runGeminiChat({ history, userMessage: request.message, draft: request.draft, system, onEvent, signal })
  } else {
    const apiKey = anthropicApiKey()
    if (!apiKey) throw createError({ statusCode: 400, message: NO_ANTHROPIC_KEY_MESSAGE })
    // The in-app assistant must not push to GitHub on its own; syncing is a user action in the sync panel.
    const bridge = await createMcpBridge({ exclude: ['sync_data'] })
    try {
      const toolNames = [...bridge.tools.map(t => t.name), 'update_draft']
      const system = buildSystemPrompt({ kind: request.kind, slug: session.slug, draft: request.draft, settings, toolNames })
      result = await runAnthropicAgent({
        messages: [...history, userMessage],
        system,
        bridge,
        model: settings.ai.anthropicModel,
        maxTokens: settings.ai.maxTokens,
        apiKey,
        onEvent,
        signal,
      })
    } finally {
      await bridge.close()
    }
  }

  const parts = [...result.parts]
  if (result.aborted) parts.push({ type: 'text', text: '（ユーザーが停止しました）' })
  else if (result.error && !parts.length) parts.push({ type: 'text', text: `エラー: ${result.error}` })
  const assistantMessage: ChatMessage = { id: assistantId, role: 'assistant', parts, createdAt: new Date().toISOString() }
  await appendChatMessage(session.id, assistantMessage)
  await touchChatSession(session.id)
  return { session, userMessage, assistantMessage, error: result.error, aborted: result.aborted }
}
