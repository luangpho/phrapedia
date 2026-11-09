import { EventEmitter } from 'node:events'
import { PassThrough } from 'node:stream'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const spawnMock = vi.hoisted(() => vi.fn())
vi.mock('node:child_process', () => ({ spawn: spawnMock }))

import { existsSync, readFileSync, statSync } from 'node:fs'
import { buildClaudeArgs, buildClaudeEnv, CHAT_DENIED_TOOLS, probeClaude, resolveCommand, runClaudeProcess, withExtraPath, writeMcpConfigFile } from '../server/utils/agent/claude-cli'

const TOOLS = ['search_sources', 'read_source', 'save_entity', 'sync_data', 'sync_status', 'update_draft', 'web_research']
const base = { model: 'claude-sonnet-5-5', systemPrompt: 'SYS', mcpUrl: 'http://127.0.0.1:4321/mcp', toolNames: TOOLS }
const valueOf = (args: string[], flag: string) => args[args.indexOf(flag) + 1]

describe('buildClaudeArgs', () => {
  it('runs headless with stream-json and partial messages', () => {
    const args = buildClaudeArgs({ ...base, sessionId: 'S' })
    expect(args[0]).toBe('-p')
    expect(valueOf(args, '--output-format')).toBe('stream-json')
    expect(args).toContain('--verbose')
    expect(args).toContain('--include-partial-messages')
    expect(valueOf(args, '--model')).toBe('claude-sonnet-5-5')
    expect(valueOf(args, '--system-prompt')).toBe('SYS')
    // the system prompt carries the live draft: it must not be frozen at the first turn of a resumed session
    expect(valueOf(args, '--system-prompt-snapshot')).toBe('off')
  })

  it('never allows sync_data and denies it explicitly', () => {
    const args = buildClaudeArgs({ ...base, sessionId: 'S' })
    const allowed = valueOf(args, '--allowedTools')!.split(',')
    expect(allowed).not.toContain('mcp__phrapedia__sync_data')
    expect(allowed).toContain('mcp__phrapedia__update_draft')
    expect(allowed).toContain('mcp__phrapedia__save_entity')
    expect(valueOf(args, '--disallowedTools')).toBe('mcp__phrapedia__sync_data')
    expect(CHAT_DENIED_TOOLS).toEqual(['sync_data'])
    // even if the caller passes it, it is filtered
    expect(allowed.every(t => t.startsWith('mcp__phrapedia__'))).toBe(true)
  })

  it('disables every built-in tool and never prompts for permission', () => {
    const args = buildClaudeArgs({ ...base, sessionId: 'S' })
    expect(valueOf(args, '--tools')).toBe('')
    expect(valueOf(args, '--permission-mode')).toBe('dontAsk')
    expect(valueOf(args, '--permission-prompts')).toBe('none')
    expect(args).not.toContain('--dangerously-skip-permissions')
  })

  it('keeps the auth header out of argv: config goes through a private 0600 file', () => {
    const input = { mcpUrl: base.mcpUrl, mcpHeaders: { Authorization: 'Basic c2VjcmV0' } }
    const file = writeMcpConfigFile(input)
    try {
      const args = buildClaudeArgs({ ...base, ...input, sessionId: 'S', mcpConfigFile: file.path })
      expect(valueOf(args, '--mcp-config')).toBe(file.path)
      expect(args.join(' ')).not.toContain('c2VjcmV0')
      expect(statSync(file.path).mode & 0o777).toBe(0o600)
      expect(JSON.parse(readFileSync(file.path, 'utf8')).mcpServers.phrapedia.headers).toEqual(input.mcpHeaders)
    } finally {
      file.cleanup()
    }
    expect(existsSync(file.path)).toBe(false)
  })

  it('points --mcp-config at the app\'s own /mcp only', () => {
    const args = buildClaudeArgs({ ...base, sessionId: 'S', mcpHeaders: { Authorization: 'Basic abc' } })
    expect(args).toContain('--strict-mcp-config')
    expect(JSON.parse(valueOf(args, '--mcp-config')!)).toEqual({
      mcpServers: { phrapedia: { type: 'http', url: 'http://127.0.0.1:4321/mcp', headers: { Authorization: 'Basic abc' } } },
    })
    expect(JSON.parse(valueOf(buildClaudeArgs({ ...base, sessionId: 'S' }), '--mcp-config')!).mcpServers.phrapedia).toEqual({ type: 'http', url: 'http://127.0.0.1:4321/mcp' })
  })

  it('uses --session-id on the first turn and --resume afterwards', () => {
    const first = buildClaudeArgs({ ...base, sessionId: '11111111-1111-4111-8111-111111111111' })
    expect(valueOf(first, '--session-id')).toBe('11111111-1111-4111-8111-111111111111')
    expect(first).not.toContain('--resume')
    const next = buildClaudeArgs({ ...base, resume: '22222222-2222-4222-8222-222222222222' })
    expect(valueOf(next, '--resume')).toBe('22222222-2222-4222-8222-222222222222')
    expect(next).not.toContain('--session-id')
  })

  it('puts the user\'s extra args right after -p and keeps the prompt out of argv', () => {
    const args = buildClaudeArgs({ ...base, sessionId: 'S', extraArgs: ['--effort', 'low'] })
    expect(args.slice(0, 3)).toEqual(['-p', '--effort', 'low'])
    expect(args[args.length - 1]).not.toBe('')
  })
})

describe('buildClaudeEnv', () => {
  it('drops the "inside Claude Code" markers, keeps the rest, and caps output tokens', () => {
    const env = buildClaudeEnv({
      PATH: '/usr/bin', HOME: '/h', CLAUDE_CONFIG_DIR: '/cfg', ANTHROPIC_API_KEY: 'k',
      CLAUDECODE: '1', CLAUDE_CODE_ENTRYPOINT: 'cli', CLAUDE_CODE_SESSION_ID: 'x', CLAUDE_CODE_CHILD_SESSION: '1',
    }, { maxTokens: 4096 })
    for (const k of ['CLAUDECODE', 'CLAUDE_CODE_ENTRYPOINT', 'CLAUDE_CODE_SESSION_ID', 'CLAUDE_CODE_CHILD_SESSION']) expect(env[k]).toBeUndefined()
    expect(env.CLAUDE_CONFIG_DIR).toBe('/cfg') // login location must survive
    expect(env.ANTHROPIC_API_KEY).toBe('k') // not touched by the app: the CLI decides
    expect(env.HOME).toBe('/h')
    expect(env.CLAUDE_CODE_MAX_OUTPUT_TOKENS).toBe('4096')
  })

  it('appends the usual CLI locations to PATH without duplicates', () => {
    const p = withExtraPath('/usr/bin:/opt/homebrew/bin')
    const dirs = p.split(':')
    expect(dirs.slice(0, 2)).toEqual(['/usr/bin', '/opt/homebrew/bin'])
    expect(dirs).toContain('/usr/local/bin')
    expect(dirs.filter(d => d === '/opt/homebrew/bin')).toHaveLength(1)
    expect(dirs.some(d => d.endsWith('/.local/bin'))).toBe(true)
  })
})

describe('resolveCommand', () => {
  it('keeps paths and unknown names as they are', () => {
    expect(resolveCommand('/abs/claude')).toBe('/abs/claude')
    expect(resolveCommand('definitely-not-installed-xyz')).toBe('definitely-not-installed-xyz')
  })
})

/** Fake child process: stdout is a PassThrough fed by the test; stdin collects the prompt. */
function fakeChild() {
  const child = new EventEmitter() as EventEmitter & { stdout: PassThrough; stderr: PassThrough; stdin: PassThrough; kill: ReturnType<typeof vi.fn>; written: string }
  child.stdout = new PassThrough()
  child.stderr = new PassThrough()
  child.stdin = new PassThrough()
  child.written = ''
  child.stdin.on('data', (d: Buffer) => { child.written += d.toString() })
  child.kill = vi.fn()
  return child
}

const result = (extra: object = {}) => JSON.stringify({ type: 'result', subtype: 'success', is_error: false, result: 'ok', session_id: 'SID', ...extra })

beforeEach(() => spawnMock.mockReset())

describe('runClaudeProcess', () => {
  const opts = (onEvent = vi.fn()) => ({ command: 'claude', args: ['-p'], prompt: 'こんにちは', env: { PATH: '/usr/bin' }, timeoutMs: 5000, onEvent })

  it('writes the prompt to stdin and turns stdout lines into events', async () => {
    const child = fakeChild()
    spawnMock.mockReturnValue(child)
    const onEvent = vi.fn()
    const p = runClaudeProcess(opts(onEvent))
    child.stdout.write(`${JSON.stringify({ type: 'system', subtype: 'init', session_id: 'SID' })}\n`)
    child.stdout.write(`${JSON.stringify({ type: 'assistant', message: { id: 'm', content: [{ type: 'text', text: '返事' }] } })}\n`)
    child.stdout.write(`${result()}\n`)
    child.stdout.end()
    child.emit('close', 0)
    const res = await p
    expect(child.written).toBe('こんにちは')
    expect(onEvent).toHaveBeenCalledWith({ type: 'text-delta', delta: '返事' })
    expect(res).toMatchObject({ sessionId: 'SID', finished: true, exitCode: 0, parts: [{ type: 'text', text: '返事' }] })
    const [cmd, args, o] = spawnMock.mock.calls[0]!
    expect(cmd).toMatch(/claude$/) // bare names are resolved to an absolute path when found
    expect(args).toEqual(['-p'])
    expect(o.stdio).toEqual(['pipe', 'pipe', 'pipe'])
    expect(o.cwd).not.toContain('phrapedia/data') // dedicated work dir, not the dataset
  })

  it('reports stderr and exit code of a failing run', async () => {
    const child = fakeChild()
    spawnMock.mockReturnValue(child)
    const p = runClaudeProcess(opts())
    child.stderr.write('No conversation found with session ID: x\n')
    child.stdout.end()
    await new Promise(r => setImmediate(r))
    child.emit('close', 1)
    const res = await p
    expect(res.exitCode).toBe(1)
    expect(res.finished).toBe(false)
    expect(res.stderr).toContain('No conversation found')
  })

  it('reports a spawn failure (CLI not installed)', async () => {
    const child = fakeChild()
    spawnMock.mockReturnValue(child)
    const p = runClaudeProcess(opts())
    child.emit('error', new Error('spawn claude ENOENT'))
    expect((await p).spawnError).toContain('ENOENT')
  })

  it('SIGTERMs on abort, escalates to SIGKILL, and flags the run as aborted', async () => {
    vi.useFakeTimers()
    try {
      const child = fakeChild()
      spawnMock.mockReturnValue(child)
      const ac = new AbortController()
      const p = runClaudeProcess({ ...opts(), signal: ac.signal })
      child.stdout.write(`${JSON.stringify({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 't', name: 'mcp__phrapedia__web_research', input: {} }] } })}\n`)
      await vi.advanceTimersByTimeAsync(10)
      ac.abort()
      expect(child.kill).toHaveBeenCalledWith('SIGTERM')
      await vi.advanceTimersByTimeAsync(5001)
      expect(child.kill).toHaveBeenCalledWith('SIGKILL')
      child.stdout.end()
      child.emit('close', null)
      const res = await p
      expect(res.aborted).toBe(true)
      expect(res.parts[0]).toMatchObject({ type: 'tool', state: 'done', isError: true })
    } finally {
      vi.useRealTimers()
    }
  })

  it('does not even spawn when already aborted', async () => {
    const ac = new AbortController()
    ac.abort()
    const res = await runClaudeProcess({ ...opts(), signal: ac.signal })
    expect(res.aborted).toBe(true)
    expect(spawnMock).not.toHaveBeenCalled()
  })

  it('kills the process when the timeout elapses', async () => {
    vi.useFakeTimers()
    try {
      const child = fakeChild()
      spawnMock.mockReturnValue(child)
      const p = runClaudeProcess({ ...opts(), timeoutMs: 1000 })
      await vi.advanceTimersByTimeAsync(1001)
      expect(child.kill).toHaveBeenCalledWith('SIGTERM')
      child.stdout.end()
      child.emit('close', null)
      expect((await p).timedOut).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('probeClaude', () => {
  it('reads the version from `claude --version` and caches it', async () => {
    const child = fakeChild()
    spawnMock.mockReturnValue(child)
    const p = probeClaude('claude-probe-a', { force: true })
    child.stdout.write('2.1.317 (Claude Code)\n')
    child.emit('close', 0)
    expect(await p).toEqual({ found: true, command: 'claude-probe-a', version: '2.1.317 (Claude Code)' })
    expect(spawnMock.mock.calls[0]![1]).toEqual(['--version'])
    await probeClaude('claude-probe-a')
    expect(spawnMock).toHaveBeenCalledTimes(1)
  })

  it('reports not found when the command cannot start', async () => {
    const child = fakeChild()
    spawnMock.mockReturnValue(child)
    const p = probeClaude('claude-probe-b', { force: true })
    child.emit('error', new Error('spawn claude-probe-b ENOENT'))
    expect(await p).toMatchObject({ found: false, error: expect.stringContaining('ENOENT') })
  })
})
