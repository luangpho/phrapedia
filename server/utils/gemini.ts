import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync } from 'node:fs'
import { copyFile, mkdir, readFile, rm } from 'node:fs/promises'
import { basename, extname, join } from 'node:path'

export interface GeminiRunOptions {
  /** Prompt sent with -p */
  prompt: string
  /**
   * Primary-source images (paths relative to the sources dir) to hand to the CLI together with the prompt.
   * They are copied into the run's work dir; the prompt must reference them (see buildResearchPrompt).
   */
  images?: string[]
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
 *   `--output-format json`, optional `--model`, and the approval mode fixed to `--mode auto`.
 * - Gemini CLI (`gemini`): the prompt is sent on stdin and `-p` carries a short trailer;
 *   `--output-format json`, optional `-m`, approval mode fixed to `--approval-mode auto_edit` (its "auto"), `--skip-trust`.
 * The approval mode is always "auto" (not user-configurable); extra args may still override the flag explicitly.
 */
export function buildCliInvocation(engine: ResearchEngine, opts: { model: string; extraArgs: string[]; prompt: string }): { args: string[]; stdin?: string } {
  const has = (flag: string) => opts.extraArgs.includes(flag)
  const args: string[] = []
  if (engine === 'antigravity') {
    if (opts.model && !has('--model')) args.push('--model', opts.model)
    args.push(...opts.extraArgs)
    if (!has('--output-format')) args.push('--output-format', 'json')
    if (!has('--mode') && !has('--dangerously-skip-permissions')) args.push('--mode', 'auto')
    args.push('-p', opts.prompt)
    return { args }
  }
  if (opts.model && !has('-m') && !has('--model')) args.push('-m', opts.model)
  args.push(...opts.extraArgs)
  if (!has('--output-format')) args.push('--output-format', 'json')
  if (!has('--approval-mode') && !has('--yolo') && !has('-y')) args.push('--approval-mode', 'auto_edit')
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
  const attachments = await stageImages(opts.images ?? [])
  const key = createHash('sha256').update(`${model}\n${attachments.map(a => a.hash).join(',')}\n${opts.prompt}`).digest('hex')
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

  // Gemini CLI embeds `@file` references (images included) straight into the prompt; Antigravity CLI reads them
  // with its file tool when told to. Both get the explicit file list plus the @ references.
  const prompt = attachments.length ? `${opts.prompt}\n\n添付画像ファイル（作業ディレクトリ直下）: ${attachments.map(a => `@${a.name}`).join(' ')}` : opts.prompt
  const { args, stdin } = buildCliInvocation(engine, { model, extraArgs: s.gemini.args ?? [], prompt })

  const started = Date.now()
  let stdout: string, stderr: string, code: number | null
  try {
    ;({ stdout, stderr, code } = await run(command, args, { timeoutMs, stdin, signal: opts.signal, workDir: attachments.length ? attachments[0]!.dir : undefined }))
  } finally {
    if (attachments.length) await rm(attachments[0]!.dir, { recursive: true, force: true }).catch(() => {})
  }
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

interface StagedImage {
  /** per-run work directory holding the copies */
  dir: string
  /** file name inside the work dir */
  name: string
  /** original path relative to the sources dir */
  source: string
  hash: string
}

const IMAGE_EXTS = new Set(['.png', '.jpg', '.jpeg', '.gif', '.webp'])

/**
 * Copy primary-source images into a fresh per-run work directory (the CLI runs with that dir as cwd, so it can
 * open the files with its own read/image tools). Paths are validated against the sources dir.
 */
async function stageImages(paths: string[]): Promise<StagedImage[]> {
  if (!paths.length) return []
  const base = await sourcesDir()
  // Validate and read everything first so that a bad path leaves no work dir behind.
  const files: { rel: string; full: string; ext: string; data: Buffer }[] = []
  for (const rel of paths.slice(0, 8)) {
    const full = safeJoin(base, rel)
    const ext = extname(full).toLowerCase()
    if (!IMAGE_EXTS.has(ext)) throw createError({ statusCode: 400, statusMessage: 'Not an image', message: `画像ファイルではありません: ${rel}` })
    let data: Buffer
    try {
      data = await readFile(full)
    } catch {
      throw createError({ statusCode: 404, statusMessage: 'Not found', message: `画像が見つかりません: ${rel}` })
    }
    files.push({ rel, full, ext, data })
  }
  const dir = join(stateDir(), 'gemini-work', `run-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`)
  await mkdir(dir, { recursive: true })
  const out: StagedImage[] = []
  try {
    for (const [i, f] of files.entries()) {
      const name = `image-${i + 1}${f.ext}`
      await copyFile(f.full, join(dir, name))
      out.push({ dir, name, source: f.rel, hash: createHash('sha256').update(f.data).digest('hex').slice(0, 16) })
    }
  } catch (err) {
    await rm(dir, { recursive: true, force: true }).catch(() => {})
    throw err
  }
  return out
}

function run(command: string, args: string[], opts: { timeoutMs: number; stdin?: string; signal?: AbortSignal; workDir?: string }): Promise<{ stdout: string; stderr: string; code: number | null }> {
  return new Promise((resolvePromise, reject) => {
    if (opts.signal?.aborted) {
      reject(createError({ statusCode: 499, statusMessage: 'Aborted', message: 'CLI call was aborted' }))
      return
    }
    // Run inside an empty work dir so the project's own GEMINI.md / AGENTS.md / .gemini settings are not picked up.
    const cwd = opts.workDir ?? join(stateDir(), 'gemini-work')
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

export interface ResearchPromptOptions {
  /** Known facts from primary sources (take precedence over the web) */
  context?: string
  /** Primary-source image paths (relative to the sources dir) that accompany the question */
  images?: string[]
  /** Text that the requesting assistant (Claude) already read from the images — reference only, not ground truth */
  ocrText?: string
}

/** Build the research prompt used by the web_research tool. */
export function buildResearchPrompt(query: string, options: ResearchPromptOptions | string = {}): string {
  const opts: ResearchPromptOptions = typeof options === 'string' ? { context: options } : options
  const images = opts.images ?? []
  const parts = [
    'あなたはタイ仏教（高僧・寺院・プラクルアン）の調査アシスタントです。',
    '必ず Google 検索ツールでウェブを検索し、以下の質問に日本語で答えてください。',
    '出力は Markdown。事実ごとに出典URLを [タイトル](URL) 形式で明記し、確認できない事項は「未確認」と書いてください。',
    'タイ語の固有名詞は原語（タイ語）と、分かればローマ字表記も併記してください。',
  ]
  if (images.length) {
    parts.push(
      '',
      '## 画像の取り扱い（必須）',
      `この質問には画像 ${images.length} 件が添付されています（作業ディレクトリ直下の ${images.map((_, i) => `image-${i + 1}`).join(', ')}。元ファイル: ${images.join(', ')}）。`,
      '1. 添付画像本体を必ず自分で開いて確認してください（ファイル読み取りツールで画像を読む）。画像を見ずに回答してはいけません。',
      '2. 画像内の文字（タイ語・日本語・英語・数字・梵字など）を必ず自分で読み取ってください。下の「参考: 事前に読み取られた文字」はあくまで参考情報であり、確定ではありません。自分の読み取り結果と照合し、相違があれば両方を示してください。',
      '3. 読み取った文字と、参考テキストや検索結果の名称・寺院名・年号などが完全に一致すると確証できない場合は、画像を拡大する、文字の部分ごとに切り出して読み直す、向きを変えて確認するなどして必ず読み直しを行ってください。それでも不確かな部分は候補を複数挙げ、確信度（高・中・低）を付けてください。',
      '4. 読み取れた文字列（原語）をそのまま検索語にして Google 検索し、画像の対象（高僧・寺院・プラクルアンの名称、寺院、製作年、種類など）を特定してください。',
      '5. 回答の冒頭に「## 画像から読み取った文字」の節を置き、読み取り結果・確信度・参考テキストとの差異を記載してから、調査結果を書いてください。',
    )
    if (opts.ocrText?.trim()) parts.push('', '### 参考: 事前に読み取られた文字（参考情報・要照合）', opts.ocrText.trim())
  }
  if (opts.context) parts.push('', '## 既知の情報（一次資料より・これを優先）', opts.context)
  parts.push('', '## 質問', query)
  return parts.join('\n')
}
