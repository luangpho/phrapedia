import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'

export interface GeminiRunOptions {
  /** Prompt sent with -p */
  prompt: string
  /** Override settings.gemini.model */
  model?: string
  timeoutMs?: number
  /** Bypass the research cache (neither read nor written) */
  noCache?: boolean
  /** Abort (kills the CLI process) when the caller goes away */
  signal?: AbortSignal
}

export interface GeminiRunResult {
  text: string
  /** Engine label (e.g. "Antigravity CLI (agy)") */
  engine?: string
  model?: string
  cached: boolean
  durationMs: number
  /** Raw stderr (trimmed) for diagnostics */
  stderr?: string
}

export interface GeminiProbe {
  found: boolean
  version?: string
  command: string
  engine: ResearchEngine
  error?: string
}

export type ResearchEngine = 'antigravity' | 'gemini-cli'

export const ENGINE_LABELS: Record<ResearchEngine, string> = {
  'antigravity': 'Antigravity CLI (agy)',
  'gemini-cli': 'Gemini CLI (gemini)',
}

function engineOf(s: { gemini: { engine?: string } }): ResearchEngine {
  return s.gemini.engine === 'gemini-cli' ? 'gemini-cli' : 'antigravity'
}

function defaultCommand(engine: ResearchEngine): string {
  return engine === 'gemini-cli' ? 'gemini' : 'agy'
}

/**
 * Build the argv for a headless run.
 * - Antigravity CLI (`agy`): the prompt goes in `-p` (agy does not read stdin when -p is given);
 *   `--output-format json`, optional `--model`, `--mode accept-edits` / `--dangerously-skip-permissions`.
 * - Gemini CLI (`gemini`): the prompt is sent on stdin and `-p` carries a short trailer;
 *   `--output-format json`, optional `-m`, `--approval-mode`, `--skip-trust`.
 */
export function buildCliInvocation(engine: ResearchEngine, opts: { model: string; extraArgs: string[]; approvalMode: string; prompt: string }): { args: string[]; stdin?: string } {
  const has = (flag: string) => opts.extraArgs.includes(flag)
  const args: string[] = []
  if (engine === 'antigravity') {
    if (opts.model && !has('--model')) args.push('--model', opts.model)
    args.push(...opts.extraArgs)
    if (!has('--output-format')) args.push('--output-format', 'json')
    if (!has('--mode') && !has('--dangerously-skip-permissions')) {
      if (opts.approvalMode === 'auto_edit') args.push('--mode', 'accept-edits')
      else if (opts.approvalMode === 'yolo') args.push('--dangerously-skip-permissions')
    }
    args.push('-p', opts.prompt)
    return { args }
  }
  if (opts.model && !has('-m') && !has('--model')) args.push('-m', opts.model)
  args.push(...opts.extraArgs)
  if (!has('--output-format')) args.push('--output-format', 'json')
  if (!has('--approval-mode') && !has('--yolo') && !has('-y')) args.push('--approval-mode', opts.approvalMode || 'default')
  if (!has('--skip-trust')) args.push('--skip-trust')
  args.push('-p', '上記の指示に従って回答してください。')
  return { args, stdin: opts.prompt }
}

/** Check that the configured CLI binary runs (`agy --version` / `gemini --version`). */
export async function probeGemini(): Promise<GeminiProbe> {
  const s = await getSettings()
  const engine = engineOf(s)
  const command = s.gemini.command || defaultCommand(engine)
  try {
    const { stdout, code, stderr } = await run(command, ['--version'], { timeoutMs: 15_000 })
    if (code !== 0) return { found: false, command, engine, error: (stderr || stdout).trim().slice(0, 400) }
    return { found: true, command, engine, version: stdout.trim().split('\n')[0] }
  } catch (err) {
    return { found: false, command, engine, error: (err as Error).message }
  }
}

/**
 * Run the configured Google CLI headlessly and return the model's answer.
 * Web search is performed by the CLI's built-in search tool (`search_web` / `google_web_search`) when the
 * prompt asks for it. The JSON envelope (`{ "response": "..." }`) is unwrapped; plain text is returned as-is.
 */
export async function runGemini(opts: GeminiRunOptions): Promise<GeminiRunResult> {
  const s = await getSettings()
  const engine = engineOf(s)
  const command = s.gemini.command || defaultCommand(engine)
  const model = opts.model ?? s.gemini.model
  const timeoutMs = opts.timeoutMs ?? s.gemini.timeoutMs
  const cacheHours = s.gemini.cacheHours
  const key = createHash('sha256').update(`${model}\n${opts.prompt}`).digest('hex')
  const db = await useDb()

  if (!opts.noCache && cacheHours > 0) {
    const cached = await db.query<{ result: string; model: string | null; created_at: string }>(
      'SELECT result, model, created_at FROM research_cache WHERE query_hash = $1', [key],
    )
    const row = cached.rows[0]
    if (row && Date.now() - new Date(row.created_at).getTime() < cacheHours * 3_600_000) {
      return { text: row.result, engine: ENGINE_LABELS[engine], model: row.model ?? undefined, cached: true, durationMs: 0 }
    }
  }

  const { args, stdin } = buildCliInvocation(engine, { model, extraArgs: s.gemini.args ?? [], approvalMode: s.gemini.approvalMode || 'default', prompt: opts.prompt })

  const started = Date.now()
  const { stdout, stderr, code } = await run(command, args, { timeoutMs, stdin, signal: opts.signal })
  const durationMs = Date.now() - started
  const label = ENGINE_LABELS[engine]
  if (code !== 0 && !stdout.trim()) {
    throw createError({ statusCode: 502, statusMessage: `${label} failed`, message: `${command} exited with code ${code}${exitHint(engine, code)}: ${stderr.trim().slice(-2000)}` })
  }
  let text: string
  try {
    text = unwrapGeminiOutput(stdout)
  } catch (err) {
    const e = err as { statusCode?: number; message?: string }
    if (e.statusCode) throw createError({ statusCode: 502, statusMessage: `${label} error`, message: `${e.message}${exitHint(engine, code)}` })
    throw err
  }
  if (!text.trim()) {
    throw createError({ statusCode: 502, statusMessage: `${label} returned no output`, message: (stderr.trim().slice(0, 2000) || 'empty response') + exitHint(engine, code) })
  }
  if (!opts.noCache && cacheHours > 0) {
    await db.query(
      `INSERT INTO research_cache (query_hash, query, result, model, created_at) VALUES ($1, $2, $3, $4, now())
       ON CONFLICT (query_hash) DO UPDATE SET result = EXCLUDED.result, model = EXCLUDED.model, created_at = now()`,
      [key, opts.prompt, text, model || null],
    )
  }
  return { text, engine: label, model: model || undefined, cached: false, durationMs, stderr: stderr.trim().slice(0, 1000) || undefined }
}

/** Human hint for well-known exit codes of both CLIs. */
function exitHint(engine: ResearchEngine, code: number | null): string {
  if (engine === 'gemini-cli') {
    if (code === 41) return ' (認証エラー: GEMINI_API_KEY を設定するか、このマシンで一度 `gemini` を実行してログインしてください)'
    if (code === 42) return ' (入力エラー)'
    return ''
  }
  if (code === 3) return ' (モデル/エージェントのエラー、または --print-timeout による打ち切り)'
  if (code === 1) return ' (起動エラー: 未ログインの可能性。`agy` を一度対話起動して Google にサインインするか、GEMINI_API_KEY と settings.json の modelProvider: "gemini" を設定してください)'
  return ''
}

/**
 * Accepts the JSON envelope of either CLI (possibly preceded by log lines) or raw text.
 * - Gemini CLI: `{ session_id, response, stats, error? }`
 * - Antigravity CLI: `{ conversation_id, status, response, error?, finished_reason? }`
 */
export function unwrapGeminiOutput(stdout: string): string {
  const trimmed = stdout.trim()
  const start = trimmed.indexOf('{')
  if (start >= 0) {
    const candidate = trimmed.slice(start)
    try {
      const parsed = JSON.parse(candidate) as { response?: unknown; result?: unknown; text?: unknown; error?: { message?: string } | string; finished_reason?: string }
      if (parsed && typeof parsed === 'object') {
        const answer = [parsed.response, parsed.result, parsed.text].find((v): v is string => typeof v === 'string')
        if (parsed.error && !answer?.trim()) {
          const msg = typeof parsed.error === 'string' ? parsed.error : parsed.error.message
          throw createError({ statusCode: 502, statusMessage: 'CLI error', message: msg || 'unknown error' })
        }
        if (typeof answer === 'string') {
          const reason = parsed.finished_reason
          return reason && reason !== 'stop' ? `${answer}\n\n（注: CLI の応答は途中で終了しました: ${reason}）` : answer
        }
      }
    } catch (err) {
      if ((err as { statusCode?: number }).statusCode) throw err
      // not JSON — fall through
    }
  }
  return trimmed
}

function run(command: string, args: string[], opts: { timeoutMs: number; stdin?: string; signal?: AbortSignal }): Promise<{ stdout: string; stderr: string; code: number | null }> {
  return new Promise((resolvePromise, reject) => {
    if (opts.signal?.aborted) {
      reject(createError({ statusCode: 499, statusMessage: 'Aborted', message: 'CLI call was aborted' }))
      return
    }
    // Run inside an empty work dir so the project's own GEMINI.md / AGENTS.md / .gemini settings are not picked up.
    const cwd = join(stateDir(), 'gemini-work')
    mkdirSync(cwd, { recursive: true })
    const child = spawn(command, args, {
      cwd,
      env: {
        ...process.env,
        NO_COLOR: '1',
        CI: 'true',
        GEMINI_CLI_NO_RELAUNCH: '1',
        GEMINI_CLI_TRUST_WORKSPACE: 'true',
        GEMINI_CLI_SURFACE: 'phrapedia',
      },
      // Antigravity CLI must not see an open stdin pipe (it may wait on it); Gemini CLI reads the prompt from it.
      stdio: [opts.stdin === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'],
    })
    if (opts.stdin !== undefined && child.stdin) {
      child.stdin.on('error', () => { /* child exited early */ })
      child.stdin.end(opts.stdin)
    }
    let stdout = ''
    let stderr = ''
    const terminate = () => {
      child.kill('SIGTERM')
      setTimeout(() => child.kill('SIGKILL'), 5_000).unref()
    }
    const timer = setTimeout(() => {
      terminate()
      reject(createError({ statusCode: 504, statusMessage: 'CLI timeout', message: `${command} did not finish within ${opts.timeoutMs} ms` }))
    }, opts.timeoutMs)
    const onAbort = () => {
      clearTimeout(timer)
      terminate()
      reject(createError({ statusCode: 499, statusMessage: 'Aborted', message: 'CLI call was aborted' }))
    }
    opts.signal?.addEventListener('abort', onAbort, { once: true })
    child.stdout?.on('data', (d: Buffer) => { stdout += d.toString('utf8') })
    child.stderr?.on('data', (d: Buffer) => { stderr += d.toString('utf8') })
    child.on('error', (err) => {
      clearTimeout(timer)
      opts.signal?.removeEventListener('abort', onAbort)
      reject(createError({ statusCode: 500, statusMessage: 'CLI not found', message: `Could not start "${command}": ${err.message}（Antigravity CLI: curl -fsSL https://antigravity.google/cli/install.sh | bash / Gemini CLI: npm i -g @google/gemini-cli）` }))
    })
    child.on('close', (code) => {
      clearTimeout(timer)
      opts.signal?.removeEventListener('abort', onAbort)
      resolvePromise({ stdout, stderr, code })
    })
  })
}

/** Build the research prompt used by the web_research tool. */
export function buildResearchPrompt(query: string, context?: string): string {
  return [
    'あなたはタイ仏教（高僧・寺院・プラクルアン）の調査アシスタントです。',
    '必ず Google 検索ツールでウェブを検索し、以下の質問に日本語で答えてください。',
    '出力は Markdown。事実ごとに出典URLを [タイトル](URL) 形式で明記し、確認できない事項は「未確認」と書いてください。',
    'タイ語の固有名詞は原語（タイ語）と、分かればローマ字表記も併記してください。',
    context ? `\n## 既知の情報（一次資料より・これを優先）\n${context}` : '',
    `\n## 質問\n${query}`,
  ].filter(Boolean).join('\n')
}
