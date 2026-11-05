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
  error?: string
}

/** Check that the configured Gemini CLI binary runs (`gemini --version`). */
export async function probeGemini(): Promise<GeminiProbe> {
  const s = await getSettings()
  const command = s.gemini.command || 'gemini'
  try {
    const { stdout, code, stderr } = await run(command, ['--version'], { timeoutMs: 15_000 })
    if (code !== 0) return { found: false, command, error: (stderr || stdout).trim().slice(0, 400) }
    return { found: true, command, version: stdout.trim().split('\n')[0] }
  } catch (err) {
    return { found: false, command, error: (err as Error).message }
  }
}

/**
 * Run the Gemini CLI headlessly (`gemini [-m model] [...args] --output-format json -p <prompt>`).
 * Web search is performed by Gemini's built-in Google Search tool when the prompt asks for it.
 * The JSON envelope (`{ "response": "..." }`) is unwrapped; plain text output is returned as-is.
 */
export async function runGemini(opts: GeminiRunOptions): Promise<GeminiRunResult> {
  const s = await getSettings()
  const command = s.gemini.command || 'gemini'
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
      return { text: row.result, model: row.model ?? undefined, cached: true, durationMs: 0 }
    }
  }

  // The prompt itself is sent on stdin (no argv length limit); `-p` carries a short trailer which also
  // forces headless mode. The CLI prepends stdin content, then a blank line, then the -p text.
  const args: string[] = []
  if (model) args.push('-m', model)
  args.push(...(s.gemini.args ?? []))
  if (!args.includes('--output-format')) args.push('--output-format', 'json')
  if (!args.includes('--approval-mode') && !args.includes('--yolo') && !args.includes('-y')) {
    args.push('--approval-mode', s.gemini.approvalMode || 'auto_edit')
  }
  if (!args.includes('--skip-trust')) args.push('--skip-trust')
  args.push('-p', '上記の指示に従って回答してください。')

  const started = Date.now()
  const { stdout, stderr, code } = await run(command, args, { timeoutMs, stdin: opts.prompt, signal: opts.signal })
  const durationMs = Date.now() - started
  if (code !== 0 && !stdout.trim()) {
    const hint = code === 41
      ? ' (認証エラー: GEMINI_API_KEY を設定するか、このマシンで一度 `gemini` を実行してログインしてください)'
      : code === 42 ? ' (入力エラー)' : ''
    throw createError({ statusCode: 502, statusMessage: 'Gemini CLI failed', message: `gemini exited with code ${code}${hint}: ${stderr.trim().slice(-2000)}` })
  }
  const text = unwrapGeminiOutput(stdout)
  if (!text.trim()) {
    throw createError({ statusCode: 502, statusMessage: 'Gemini CLI returned no output', message: stderr.trim().slice(0, 2000) || 'empty response' })
  }
  if (!opts.noCache && cacheHours > 0) {
    await db.query(
      `INSERT INTO research_cache (query_hash, query, result, model, created_at) VALUES ($1, $2, $3, $4, now())
       ON CONFLICT (query_hash) DO UPDATE SET result = EXCLUDED.result, model = EXCLUDED.model, created_at = now()`,
      [key, opts.prompt, text, model || null],
    )
  }
  return { text, model: model || undefined, cached: false, durationMs, stderr: stderr.trim().slice(0, 1000) || undefined }
}

/** Accepts `{response: string}` JSON (possibly preceded by log lines) or raw text. */
export function unwrapGeminiOutput(stdout: string): string {
  const trimmed = stdout.trim()
  const start = trimmed.indexOf('{')
  if (start >= 0) {
    const candidate = trimmed.slice(start)
    try {
      const parsed = JSON.parse(candidate) as { response?: unknown; error?: { message?: string } | string }
      if (parsed && typeof parsed === 'object') {
        if (typeof parsed.response === 'string') return parsed.response
        if (parsed.error) {
          const msg = typeof parsed.error === 'string' ? parsed.error : parsed.error.message
          throw createError({ statusCode: 502, statusMessage: 'Gemini CLI error', message: msg || 'unknown error' })
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
      reject(createError({ statusCode: 499, statusMessage: 'Aborted', message: 'Gemini CLI call was aborted' }))
      return
    }
    // Run inside an empty work dir so the project's own GEMINI.md / .gemini settings are not picked up.
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
      stdio: ['pipe', 'pipe', 'pipe'],
    })
    child.stdin.on('error', () => { /* child exited early */ })
    child.stdin.end(opts.stdin ?? '')
    let stdout = ''
    let stderr = ''
    const terminate = () => {
      child.kill('SIGTERM')
      setTimeout(() => child.kill('SIGKILL'), 5_000).unref()
    }
    const timer = setTimeout(() => {
      terminate()
      reject(createError({ statusCode: 504, statusMessage: 'Gemini CLI timeout', message: `gemini did not finish within ${opts.timeoutMs} ms` }))
    }, opts.timeoutMs)
    const onAbort = () => {
      clearTimeout(timer)
      terminate()
      reject(createError({ statusCode: 499, statusMessage: 'Aborted', message: 'Gemini CLI call was aborted' }))
    }
    opts.signal?.addEventListener('abort', onAbort, { once: true })
    child.stdout.on('data', (d: Buffer) => { stdout += d.toString('utf8') })
    child.stderr.on('data', (d: Buffer) => { stderr += d.toString('utf8') })
    child.on('error', (err) => {
      clearTimeout(timer)
      opts.signal?.removeEventListener('abort', onAbort)
      reject(createError({ statusCode: 500, statusMessage: 'Gemini CLI not found', message: `Could not start "${command}": ${err.message}` }))
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
