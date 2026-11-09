import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ChatMessage, ChatSession, ChatStreamEvent } from '../shared/types/chat'
import { DEFAULT_SETTINGS } from '../shared/types/settings'

const runMock = vi.hoisted(() => vi.fn())
const probeMock = vi.hoisted(() => vi.fn())
vi.mock('../server/utils/agent/claude-cli', async (orig) => {
  const actual = await orig<typeof import('../server/utils/agent/claude-cli')>()
  return { ...actual, runClaudeProcess: runMock, probeClaude: probeMock }
})

import { CLAUDE_LOGIN_MESSAGE, describeClaudeFailure, mcpAuthHeaders, NO_CLAUDE_CLI_MESSAGE, prepareChatTurn, runChatTurn } from '../server/utils/agent'
import { summarizeHistory, withHistoryDigest } from '../server/utils/agent/history'

const g = globalThis as Record<string, unknown>
const opts = { command: 'claude', timeoutMs: 600_000, model: 'claude-sonnet-5-5' }
const fail = (extra: object = {}) => ({ timedOut: undefined, spawnError: undefined, resultError: undefined, stderr: '', exitCode: 1, finished: false, ...extra })

describe('describeClaudeFailure', () => {
  it('explains a missing CLI', () => {
    expect(describeClaudeFailure(fail({ spawnError: 'spawn claude ENOENT' }), opts)).toContain('claude ENOENT')
    expect(describeClaudeFailure(fail({ spawnError: 'x' }), opts)).toContain('Claude Code CLI（claude）が見つかりません')
  })
  it('explains a missing login', () => {
    expect(describeClaudeFailure(fail({ resultError: 'Not logged in · Please run /login' }), opts)).toBe(CLAUDE_LOGIN_MESSAGE)
    expect(describeClaudeFailure(fail({ stderr: 'Invalid API key' }), opts)).toBe(CLAUDE_LOGIN_MESSAGE)
  })
  it('explains a timeout, a bad model and limits', () => {
    expect(describeClaudeFailure(fail({ timedOut: true }), { ...opts, timeoutMs: 90_000 })).toContain('90 秒')
    expect(describeClaudeFailure(fail({ resultError: 'There\'s an issue with the selected model (nope). It may not exist' }), { ...opts, model: 'nope' })).toContain('モデル「nope」')
    expect(describeClaudeFailure(fail({ resultError: 'Claude usage limit reached' }), opts)).toContain('利用上限')
  })
  it('falls back to the CLI text or the exit code', () => {
    expect(describeClaudeFailure(fail({ resultError: 'something odd' }), opts)).toBe('Claude Code CLI のエラー: something odd')
    expect(describeClaudeFailure(fail({ exitCode: 7 }), opts)).toContain('exit code 7')
  })
})

describe('mcpAuthHeaders', () => {
  it('is empty unless Basic auth is configured', () => {
    expect(mcpAuthHeaders({})).toBeUndefined()
    expect(mcpAuthHeaders({ basicAuthUser: 'u', basicAuthPassword: '' })).toBeUndefined()
    expect(mcpAuthHeaders({ basicAuthUser: 'u', basicAuthPassword: 'p' })).toEqual({ Authorization: `Basic ${Buffer.from('u:p').toString('base64')}` })
  })
})

describe('history digest', () => {
  const msg = (role: 'user' | 'assistant', parts: ChatMessage['parts']): ChatMessage => ({ id: Math.random().toString(), role, parts, createdAt: '' })
  it('renders text, tools and drafts; skips system messages', () => {
    const digest = summarizeHistory([
      msg('user', [{ type: 'text', text: '調べて' }]),
      msg('assistant', [{ type: 'tool', toolCallId: 't', name: 'search_sources', input: {}, state: 'done' }, { type: 'draft', markdown: '#', summary: '追加' }, { type: 'text', text: '完了' }]),
      { ...msg('user', [{ type: 'text', text: 'hidden' }]), role: 'system' },
    ])
    expect(digest).toBe('ユーザー: 調べて\nアシスタント: （ツール search_sources を実行）\n（update_draft で下書きを更新しました: 追加）\n完了')
  })
  it('keeps the newest messages when too long and returns the bare prompt without history', () => {
    const many = Array.from({ length: 100 }, (_, i) => msg('user', [{ type: 'text', text: `${i}:${'あ'.repeat(1000)}` }]))
    const digest = summarizeHistory(many)
    expect(digest.length).toBeLessThan(26_000)
    expect(digest).toContain('99:')
    expect(digest).not.toContain('\nユーザー: 0:')
    expect(withHistoryDigest('次の質問', [])).toBe('次の質問')
    expect(withHistoryDigest('次の質問', many.slice(0, 1))).toMatch(/<conversation_so_far>[\s\S]*<\/conversation_so_far>[\s\S]*次の質問$/)
  })
})

describe('runChatTurn: session handling', () => {
  let session: ChatSession
  let stored: ChatMessage[]
  let savedIds: Array<string | null>
  const events: ChatStreamEvent[] = []

  const okRun = (sessionId: string, text = '返事') => ({
    parts: [{ type: 'text', text }], sessionId, finished: true, exitCode: 0, stderr: '',
  })

  beforeEach(() => {
    runMock.mockReset()
    probeMock.mockReset()
    probeMock.mockResolvedValue({ found: true, command: 'claude', version: '2.1.317' })
    events.length = 0
    stored = []
    savedIds = []
    session = { id: 's1', kind: 'monks', slug: null, title: 't', claudeSessionId: null, createdAt: '', updatedAt: '' }
    g.getSettings = async () => DEFAULT_SETTINGS
    g.getChatSession = async () => session
    g.createChatSession = async () => session
    g.touchChatSession = async () => {}
    g.newChatId = () => 'new-id'
    g.listChatMessages = async () => [...stored]
    g.appendChatMessage = async (_id: string, m: ChatMessage) => { stored.push(m) }
    g.setClaudeSessionId = async (_id: string, v: string | null) => { savedIds.push(v) }
    g.listMcpToolNames = async (o: { exclude?: string[] } = {}) => ['search_sources', 'sync_data', 'update_draft'].filter(n => !o.exclude?.includes(n))
  })

  const request = { kind: 'monks' as const, message: '質問', draft: '' }
  const mcp = { url: 'http://127.0.0.1:1/mcp' }
  const run = async () => runChatTurn(request, ev => events.push(ev), undefined, await prepareChatTurn(request), mcp)
  const argsOf = (call: number) => runMock.mock.calls[call]![0].args as string[]
  const promptOf = (call: number) => runMock.mock.calls[call]![0].prompt as string

  it('rejects the turn early with a Japanese message when claude is missing', async () => {
    probeMock.mockResolvedValue({ found: false, command: 'claude', error: 'ENOENT' })
    await expect(prepareChatTurn(request)).rejects.toMatchObject({ statusCode: 400, message: NO_CLAUDE_CLI_MESSAGE })
  })

  it('first turn: --session-id with a fresh uuid, remembers the id the CLI reports', async () => {
    runMock.mockResolvedValueOnce(okRun('cli-1'))
    const res = await run()
    const args = argsOf(0)
    expect(args).toContain('--session-id')
    expect(args[args.indexOf('--session-id') + 1]).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
    expect(args).not.toContain('--resume')
    expect(args[args.indexOf('--allowedTools') + 1]).toBe('mcp__phrapedia__search_sources,mcp__phrapedia__update_draft')
    expect(promptOf(0)).toBe('質問')
    expect(savedIds).toEqual(['cli-1'])
    expect(res.assistantMessage.parts).toEqual([{ type: 'text', text: '返事' }])
    expect(stored.map(m => m.role)).toEqual(['user', 'assistant'])
  })

  it('later turns: --resume with the stored id and the plain message', async () => {
    session.claudeSessionId = 'cli-1'
    stored.push({ id: 'old', role: 'user', parts: [{ type: 'text', text: '前の質問' }], createdAt: '' })
    runMock.mockResolvedValueOnce(okRun('cli-1'))
    await run()
    expect(runMock).toHaveBeenCalledTimes(1)
    const args = argsOf(0)
    expect(args[args.indexOf('--resume') + 1]).toBe('cli-1')
    expect(args).not.toContain('--session-id')
    expect(promptOf(0)).toBe('質問')
  })

  it('resume failure: starts a new session and hands over the stored history', async () => {
    session.claudeSessionId = 'gone'
    stored.push({ id: 'old', role: 'user', parts: [{ type: 'text', text: '前の質問' }], createdAt: '' })
    runMock
      .mockResolvedValueOnce({ parts: [], finished: true, resultError: 'No conversation found with session ID: gone', exitCode: 1, stderr: '' })
      .mockResolvedValueOnce(okRun('cli-2'))
    const res = await run()
    expect(runMock).toHaveBeenCalledTimes(2)
    expect(argsOf(0)).toContain('--resume')
    expect(argsOf(1)).toContain('--session-id')
    expect(argsOf(1)).not.toContain('--resume')
    expect(promptOf(1)).toContain('前の質問')
    expect(promptOf(1).endsWith('質問')).toBe(true)
    expect(savedIds).toEqual(['cli-2'])
    expect(events.some(e => e.type === 'error')).toBe(false)
    expect(res.error).toBeUndefined()
  })

  it('does not retry other failures; emits one Japanese error event and keeps the message', async () => {
    session.claudeSessionId = 'cli-1'
    runMock.mockResolvedValueOnce({ parts: [], sessionId: 'cli-1', finished: true, resultError: 'Not logged in · Please run /login', exitCode: 1, stderr: '' })
    const res = await run()
    expect(runMock).toHaveBeenCalledTimes(1)
    expect(events.filter(e => e.type === 'error')).toEqual([{ type: 'error', message: CLAUDE_LOGIN_MESSAGE }])
    expect(res.error).toBe(CLAUDE_LOGIN_MESSAGE)
    expect(res.assistantMessage.parts).toEqual([{ type: 'text', text: `エラー: ${CLAUDE_LOGIN_MESSAGE}` }])
  })

  it('abort: keeps the partial parts and appends the stop note, no error event', async () => {
    runMock.mockResolvedValueOnce({ parts: [{ type: 'text', text: '途中' }], sessionId: 'cli-1', finished: false, aborted: true, exitCode: null, stderr: '' })
    const res = await run()
    expect(res.aborted).toBe(true)
    expect(res.assistantMessage.parts).toEqual([{ type: 'text', text: '途中' }, { type: 'text', text: '（ユーザーが停止しました）' }])
    expect(events.some(e => e.type === 'error')).toBe(false)
  })

  it('passes settings to the process: timeout, extra args, env without nesting markers', async () => {
    g.getSettings = async () => ({ ...DEFAULT_SETTINGS, ai: { ...DEFAULT_SETTINGS.ai, args: ['--effort', 'low'], timeoutMs: 1234, maxTokens: 999 } })
    process.env.CLAUDECODE = '1'
    runMock.mockResolvedValueOnce(okRun('cli-1'))
    try {
      await run()
    } finally {
      delete process.env.CLAUDECODE
    }
    const o = runMock.mock.calls[0]![0]
    expect(o.timeoutMs).toBe(1234)
    expect(o.args.slice(0, 3)).toEqual(['-p', '--effort', 'low'])
    expect(o.env.CLAUDECODE).toBeUndefined()
    expect(o.env.CLAUDE_CODE_MAX_OUTPUT_TOKENS).toBe('999')
  })
})
