import { spawn } from 'node:child_process'
import { existsSync, mkdirSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { delimiter, join } from 'node:path'
import { createInterface } from 'node:readline'
import { ClaudeStreamParser, MCP_SERVER_NAME, MCP_TOOL_PREFIX } from './claude-stream'
import type { ClaudeRunResult } from './claude-stream'
import type { ChatStreamEvent } from '../../../shared/types/chat'

/** Tools of the app's own MCP server that the chat must never call (syncing to GitHub is a user action). */
export const CHAT_DENIED_TOOLS = ['sync_data']

/** Variables that mark "running inside Claude Code"; passing them on makes a nested `claude` refuse / misbehave. */
export const CLAUDE_NESTING_ENV = [
  'CLAUDECODE',
  'CLAUDE_CODE_ENTRYPOINT',
  'CLAUDE_CODE_CHILD_SESSION',
  'CLAUDE_CODE_SESSION_ID',
  'CLAUDE_CODE_SESSION_ATTENDED',
  'CLAUDE_CODE_EXECPATH',
  'CLAUDE_CODE_MESSAGING_SOCKET',
  'CLAUDE_CODE_MESSAGING_TOKEN',
  'CLAUDE_PID',
]

export interface ClaudeArgsInput {
  model: string
  systemPrompt: string
  /** HTTP URL of this app's /mcp endpoint */
  mcpUrl: string
  /** Extra HTTP headers for /mcp (e.g. Authorization when Basic auth is enabled) */
  mcpHeaders?: Record<string, string>
  /** Names (without the mcp__phrapedia__ prefix) of the MCP tools the chat may use */
  toolNames: string[]
  /** First turn: new session id (uuid v4) */
  sessionId?: string
  /** Later turns: session to resume */
  resume?: string
  /** settings.ai.args */
  extraArgs?: string[]
}

/**
 * argv for `claude -p`. The prompt itself goes through stdin (no positional argument), so variadic options such as
 * --allowedTools / --mcp-config cannot swallow it.
 * Flags (all present in `claude --help` of 2.1.317):
 *  -p, --output-format stream-json, --verbose, --include-partial-messages, --model, --system-prompt,
 *  --system-prompt-snapshot off (the system prompt carries the live editor draft, so it must be re-sent on resume),
 *  --mcp-config + --strict-mcp-config, --tools "" (no built-in tools), --allowedTools, --disallowedTools,
 *  --permission-mode dontAsk + --permission-prompts none (never prompt: anything not allowed is denied),
 *  --setting-sources "" + --disable-slash-commands (isolate from the user's settings / hooks / skills),
 *  --session-id / --resume.
 */
export function buildClaudeArgs(input: ClaudeArgsInput): string[] {
  const allowed = input.toolNames.filter(n => !CHAT_DENIED_TOOLS.includes(n)).map(n => `${MCP_TOOL_PREFIX}${n}`)
  const denied = CHAT_DENIED_TOOLS.map(n => `${MCP_TOOL_PREFIX}${n}`)
  const mcpConfig = {
    mcpServers: {
      [MCP_SERVER_NAME]: {
        type: 'http',
        url: input.mcpUrl,
        ...(input.mcpHeaders && Object.keys(input.mcpHeaders).length ? { headers: input.mcpHeaders } : {}),
      },
    },
  }
  const args: string[] = ['-p', ...(input.extraArgs ?? [])]
  args.push(
    '--output-format', 'stream-json',
    '--verbose',
    '--include-partial-messages',
    '--model', input.model,
    '--system-prompt', input.systemPrompt,
    '--system-prompt-snapshot', 'off',
    '--mcp-config', JSON.stringify(mcpConfig),
    '--strict-mcp-config',
    '--tools', '',
    '--allowedTools', allowed.join(','),
    '--disallowedTools', denied.join(','),
    '--permission-mode', 'dontAsk',
    '--permission-prompts', 'none',
    // Keep the user's own Claude Code setup (hooks, plugins, skills, CLAUDE.md-style output rules) out of the app's chat.
    '--setting-sources', '',
    '--disable-slash-commands',
  )
  if (input.resume) args.push('--resume', input.resume)
  else if (input.sessionId) args.push('--session-id', input.sessionId)
  return args
}

/** Env for the child: inherit, drop the "inside Claude Code" markers, cap the output tokens, widen PATH. */
export function buildClaudeEnv(base: NodeJS.ProcessEnv, opts: { maxTokens: number }): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...base, NO_COLOR: '1' }
  for (const key of CLAUDE_NESTING_ENV) delete env[key]
  env.CLAUDE_CODE_MAX_OUTPUT_TOKENS = String(opts.maxTokens)
  env.PATH = withExtraPath(env.PATH)
  return env
}

/** Directories where `claude` (and the `node` it may need) usually lives; GUI-launched apps get a minimal PATH. */
export function extraPathDirs(): string[] {
  const home = homedir()
  return ['/opt/homebrew/bin', '/usr/local/bin', join(home, '.local', 'bin'), join(home, '.claude', 'local')]
}

export function withExtraPath(path: string | undefined): string {
  const have = (path ?? '').split(delimiter).filter(Boolean)
  for (const dir of extraPathDirs()) if (!have.includes(dir)) have.push(dir)
  return have.join(delimiter)
}

/** A bare command name is resolved against PATH + the usual locations; paths are used as they are. */
export function resolveCommand(command: string, pathVar: string | undefined = process.env.PATH): string {
  if (command.includes('/') || command.includes('\\')) return command
  for (const dir of withExtraPath(pathVar).split(delimiter)) {
    const candidate = join(dir, command)
    if (existsSync(candidate)) return candidate
  }
  return command
}

/** Empty working dir for the CLI so that it can neither write into the dataset nor pick up project files. */
export function claudeWorkDir(): string {
  const dir = join(tmpdir(), 'phrapedia-claude-work')
  mkdirSync(dir, { recursive: true })
  return dir
}

export interface ClaudeProbe {
  found: boolean
  version?: string
  command: string
  error?: string
}

const PROBE_TTL_MS = 60_000
const probeCache = new Map<string, { at: number; value: ClaudeProbe }>()

/** `claude --version`, cached for a minute per command. */
export async function probeClaude(command: string, opts: { force?: boolean } = {}): Promise<ClaudeProbe> {
  const cached = probeCache.get(command)
  if (!opts.force && cached && Date.now() - cached.at < PROBE_TTL_MS) return cached.value
  const value = await new Promise<ClaudeProbe>((resolvePromise) => {
    let out = ''
    let err = ''
    let settled = false
    const done = (v: ClaudeProbe) => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      resolvePromise(v)
    }
    const child = spawn(resolveCommand(command), ['--version'], {
      cwd: claudeWorkDir(),
      env: buildClaudeEnv(process.env, { maxTokens: 1 }),
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      done({ found: false, command, error: '`--version` が 15 秒以内に終わりませんでした' })
    }, 15_000)
    child.stdout?.on('data', (d: Buffer) => { out += d.toString('utf8') })
    child.stderr?.on('data', (d: Buffer) => { err += d.toString('utf8') })
    child.on('error', e => done({ found: false, command, error: e.message }))
    child.on('close', (code) => {
      if (code === 0) done({ found: true, command, version: out.trim().split('\n')[0] })
      else done({ found: false, command, error: (err || out).trim().slice(0, 400) || `exit code ${code}` })
    })
  })
  probeCache.set(command, { at: Date.now(), value })
  return value
}

export interface ClaudeProcessOptions {
  command: string
  args: string[]
  /** User prompt, written to stdin */
  prompt: string
  env: NodeJS.ProcessEnv
  timeoutMs: number
  onEvent: (ev: ChatStreamEvent) => void
  signal?: AbortSignal
}

export interface ClaudeProcessResult extends ClaudeRunResult {
  aborted?: boolean
  timedOut?: boolean
  /** Failed to start the process at all (ENOENT / EACCES ...) */
  spawnError?: string
  exitCode: number | null
  stderr: string
}

const KILL_GRACE_MS = 5_000
const STDERR_LIMIT = 8_000

/**
 * Spawn `claude -p`, stream its stdout through ClaudeStreamParser and resolve when the process exits.
 * Abort -> SIGTERM, SIGKILL after a grace period. Never rejects; failures are reported in the result.
 */
export function runClaudeProcess(opts: ClaudeProcessOptions): Promise<ClaudeProcessResult> {
  return new Promise((resolvePromise) => {
    const parser = new ClaudeStreamParser(opts.onEvent)
    if (opts.signal?.aborted) {
      resolvePromise({ ...parser.result(), aborted: true, exitCode: null, stderr: '' })
      return
    }
    let stderr = ''
    let aborted = false
    let timedOut = false
    let spawnError: string | undefined
    let killTimer: ReturnType<typeof setTimeout> | undefined
    let timer: ReturnType<typeof setTimeout> | undefined

    const finish = (exitCode: number | null) => {
      if (timer) clearTimeout(timer)
      if (killTimer) clearTimeout(killTimer)
      opts.signal?.removeEventListener('abort', onAbort)
      if (aborted || timedOut || !parser.finished) parser.markInterrupted()
      resolvePromise({ ...parser.result(), aborted: aborted || undefined, timedOut: timedOut || undefined, spawnError, exitCode, stderr: stderr.trim() })
    }


    const child = spawn(resolveCommand(opts.command), opts.args, {
      cwd: claudeWorkDir(),
      env: opts.env,
      stdio: ['pipe', 'pipe', 'pipe'],
    })

    const terminate = () => {
      child.kill('SIGTERM')
      killTimer = setTimeout(() => child.kill('SIGKILL'), KILL_GRACE_MS)
      killTimer.unref()
    }
    timer = setTimeout(() => {
      timedOut = true
      terminate()
    }, opts.timeoutMs)
    const onAbort = () => {
      aborted = true
      terminate()
    }
    opts.signal?.addEventListener('abort', onAbort, { once: true })

    child.stdin?.on('error', () => { /* child exited early */ })
    child.stdin?.end(opts.prompt)
    child.stderr?.on('data', (d: Buffer) => {
      if (stderr.length < STDERR_LIMIT) stderr += d.toString('utf8')
    })
    if (child.stdout) {
      const rl = createInterface({ input: child.stdout, crlfDelay: Infinity })
      rl.on('line', line => parser.feed(line))
    }
    child.on('error', (err) => {
      spawnError = err.message
      finish(null)
    })
    child.on('close', code => finish(code))
  })
}
