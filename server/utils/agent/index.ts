import { randomUUID } from 'node:crypto'
import type { ChatMessage, ChatRequest, ChatSession, ChatStreamEvent } from '../../../shared/types/chat'
import { buildClaudeArgs, buildClaudeEnv, CHAT_DENIED_TOOLS, probeClaude, runClaudeProcess, writeMcpConfigFile } from './claude-cli'
import type { ClaudeProcessResult } from './claude-cli'
import { withHistoryDigest } from './history'
import { buildSystemPrompt } from './prompt'

export const NO_CLAUDE_CLI_MESSAGE
  = 'Claude Code CLI（claude）が見つかりません。Claude Code をインストールし、ターミナルで `claude` を実行してログインしてください。見つからない場合は 設定 →「チャット AI」でコマンドのパス（例: /opt/homebrew/bin/claude）を指定できます。'
export const CLAUDE_LOGIN_MESSAGE
  = 'Claude Code CLI にログインしていないようです。ターミナルで `claude` を実行してログインしてから、もう一度お試しください。'

export interface ChatTurnResult {
  session: ChatSession
  userMessage: ChatMessage
  assistantMessage: ChatMessage
  /** Set when the CLI failed; an {type:'error'} event has already been emitted */
  error?: string
  aborted?: boolean
}

/** Where the CLI reaches this app's own MCP endpoint (computed per request: the Tauri sidecar port is dynamic). */
export interface ChatMcpTarget {
  url: string
  headers?: Record<string, string>
}

/** Prepare the session (create / load) and the message ids before any provider work starts. */
export async function prepareChatTurn(request: ChatRequest): Promise<{ session: ChatSession; userMessage: ChatMessage; assistantId: string }> {
  const settings = await getSettings()
  const probe = await probeClaude(settings.ai.command)
  if (!probe.found) {
    throw createError({ statusCode: 400, statusMessage: 'Claude Code CLI not found', message: NO_CLAUDE_CLI_MESSAGE })
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

/** Japanese, user-facing message for a failed CLI run. */
export function describeClaudeFailure(run: Pick<ClaudeProcessResult, 'spawnError' | 'timedOut' | 'resultError' | 'stderr' | 'exitCode' | 'finished'>, opts: { command: string; timeoutMs: number; model: string }): string {
  if (run.spawnError) return `${NO_CLAUDE_CLI_MESSAGE}（${opts.command}: ${run.spawnError}）`
  if (run.timedOut) return `Claude Code CLI が ${Math.round(opts.timeoutMs / 1000)} 秒以内に終わらなかったため中断しました。設定のタイムアウトを延ばすか、依頼を分けてお試しください。`
  const detail = (run.resultError || run.stderr || '').trim()
  if (/not logged in|please run \/login|\/login|invalid api key|authentication|unauthori[sz]ed|oauth|credential/i.test(detail)) return CLAUDE_LOGIN_MESSAGE
  if (/model/i.test(detail) && /(not found|unknown|invalid|unrecognized|does not exist|issue with the selected model)/i.test(detail)) {
    return `モデル「${opts.model}」を Claude Code CLI が受け付けませんでした。設定のモデル名を確認してください。${detail ? `（${detail.slice(0, 300)}）` : ''}`
  }
  if (/rate limit|usage limit|overloaded|429|529/i.test(detail)) return `Claude の利用上限または混雑のため応答できませんでした。しばらく待ってから再試行してください。（${detail.slice(0, 300)}）`
  if (detail) return `Claude Code CLI のエラー: ${detail.slice(0, 600)}`
  return `Claude Code CLI が異常終了しました（exit code ${run.exitCode ?? '?'}）。`
}

function isResumeFailure(run: ClaudeProcessResult): boolean {
  return !run.parts.length && /no conversation found|session.*(not found|does not exist)/i.test(`${run.resultError ?? ''}\n${run.stderr}`)
}

/**
 * Run one chat turn: store the user message, run `claude -p` (the tool-calling loop happens inside the CLI, which calls
 * back into this app's /mcp endpoint; Gemini is only reachable through the web_research tool), store the assistant
 * message and touch the session.
 */
export async function runChatTurn(
  request: ChatRequest,
  onEvent: (ev: ChatStreamEvent) => void,
  signal: AbortSignal | undefined,
  prepared: Awaited<ReturnType<typeof prepareChatTurn>> | undefined,
  mcp: ChatMcpTarget,
): Promise<ChatTurnResult> {
  const settings = await getSettings()
  const { session, userMessage, assistantId } = prepared ?? await prepareChatTurn(request)
  const history = await listChatMessages(session.id)
  await appendChatMessage(session.id, userMessage)

  const toolNames = await listMcpToolNames({ exclude: CHAT_DENIED_TOOLS })
  const system = buildSystemPrompt({ kind: request.kind, slug: session.slug, draft: request.draft, settings, toolNames })
  const env = buildClaudeEnv(process.env, { maxTokens: settings.ai.maxTokens })

  // Auth headers must not appear in argv (`ps` shows it): hand them over through a private 0600 file instead.
  const mcpFile = mcp.headers && Object.keys(mcp.headers).length ? writeMcpConfigFile({ mcpUrl: mcp.url, mcpHeaders: mcp.headers }) : undefined
  const attempt = (ids: { sessionId?: string; resume?: string }, prompt: string) => runClaudeProcess({
    command: settings.ai.command,
    args: buildClaudeArgs({
      model: settings.ai.model,
      systemPrompt: system,
      mcpUrl: mcp.url,
      mcpHeaders: mcp.headers,
      mcpConfigFile: mcpFile?.path,
      toolNames,
      extraArgs: settings.ai.args,
      ...ids,
    }),
    prompt,
    env,
    timeoutMs: settings.ai.timeoutMs,
    onEvent,
    signal,
  })

  // First turn: --session-id <new uuid>. Later turns: --resume <id>. When resume fails (other machine, session file gone)
  // start over with a new session and hand the stored history over as a digest.
  let claudeSessionId = session.claudeSessionId ?? null
  let run: ClaudeProcessResult
  try {
    if (claudeSessionId) {
      run = await attempt({ resume: claudeSessionId }, request.message)
      if (isResumeFailure(run)) {
        claudeSessionId = randomUUID()
        run = await attempt({ sessionId: claudeSessionId }, withHistoryDigest(request.message, history))
      }
    } else {
      claudeSessionId = randomUUID()
      run = await attempt({ sessionId: claudeSessionId }, history.length ? withHistoryDigest(request.message, history) : request.message)
    }
  } finally {
    mcpFile?.cleanup()
  }

  const failed = !run.aborted && (Boolean(run.spawnError) || run.timedOut || Boolean(run.resultError) || (!run.finished && run.exitCode !== 0))
  let error: string | undefined
  if (failed) {
    error = describeClaudeFailure(run, { command: settings.ai.command, timeoutMs: settings.ai.timeoutMs, model: settings.ai.model })
    onEvent({ type: 'error', message: error })
  }
  // Remember the CLI session only when the CLI actually created it.
  if (run.sessionId) await setClaudeSessionId(session.id, run.sessionId)

  const parts = [...run.parts]
  if (run.aborted) parts.push({ type: 'text', text: '（ユーザーが停止しました）' })
  else if (error && !parts.length) parts.push({ type: 'text', text: `エラー: ${error}` })
  const assistantMessage: ChatMessage = { id: assistantId, role: 'assistant', parts, createdAt: new Date().toISOString() }
  await appendChatMessage(session.id, assistantMessage)
  await touchChatSession(session.id)
  return { session, userMessage, assistantMessage, error, aborted: run.aborted }
}

/** Authorization header for /mcp when the deployment is protected with Basic auth (server/middleware/auth.ts). */
export function mcpAuthHeaders(cfg: { basicAuthUser?: unknown; basicAuthPassword?: unknown }): Record<string, string> | undefined {
  const user = String(cfg.basicAuthUser || '')
  const password = String(cfg.basicAuthPassword || '')
  if (!user || !password) return undefined
  return { Authorization: `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}` }
}
