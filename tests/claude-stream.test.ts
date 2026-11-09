import { describe, it, expect } from 'vitest'
import type { ChatStreamEvent } from '../shared/types/chat'
import { ClaudeStreamParser, EVENT_OUTPUT_LIMIT, flattenToolResult, shortToolName } from '../server/utils/agent/claude-stream'

const line = (o: unknown) => JSON.stringify(o)
const SID = '5c77479f-23ba-41cb-b2bd-99046818ffb0'

function textDelta(messageId: string, text: string) {
  return line({ type: 'stream_event', event: { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text } }, session_id: SID, parent_tool_use_id: null, api_message_id: messageId })
}
const messageStart = (id: string) => line({ type: 'stream_event', event: { type: 'message_start', message: { id, role: 'assistant', content: [] } }, session_id: SID, parent_tool_use_id: null })

function run(lines: string[]) {
  const events: ChatStreamEvent[] = []
  const parser = new ClaudeStreamParser(ev => events.push(ev))
  for (const l of lines) parser.feed(l)
  return { events, parser }
}

/** system/init -> text delta -> tool_use -> tool_result -> text delta -> result (shape of Claude Code 2.1.317) */
const FIXTURE = [
  line({ type: 'system', subtype: 'init', session_id: SID, tools: [], mcp_servers: [{ name: 'phrapedia', status: 'connected' }], model: 'claude-sonnet-5-5' }),
  line({ type: 'system', subtype: 'status', status: 'requesting', session_id: SID }),
  messageStart('msg_1'),
  textDelta('msg_1', '調べます'),
  textDelta('msg_1', '。'),
  line({ type: 'assistant', message: { id: 'msg_1', role: 'assistant', content: [{ type: 'text', text: '調べます。' }, { type: 'tool_use', id: 'toolu_1', name: 'mcp__phrapedia__search_sources', input: { query: 'ソムデット' } }] }, session_id: SID, parent_tool_use_id: null }),
  line({ type: 'user', message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: 'toolu_1', content: [{ type: 'text', text: '{"count":2}' }] }] }, session_id: SID, parent_tool_use_id: null }),
  messageStart('msg_2'),
  textDelta('msg_2', '2 件でした。'),
  line({ type: 'assistant', message: { id: 'msg_2', role: 'assistant', content: [{ type: 'text', text: '2 件でした。' }] }, session_id: SID, parent_tool_use_id: null }),
  line({ type: 'rate_limit_event', rate_limit_info: { status: 'allowed' } }),
  line({ type: 'result', subtype: 'success', is_error: false, result: '2 件でした。', session_id: SID }),
]

describe('ClaudeStreamParser', () => {
  it('converts a full turn into ChatStreamEvents in order', () => {
    const { events, parser } = run(FIXTURE)
    expect(events).toEqual([
      { type: 'text-delta', delta: '調べます' },
      { type: 'text-delta', delta: '。' },
      { type: 'tool-start', toolCallId: 'toolu_1', name: 'search_sources', input: { query: 'ソムデット' } },
      { type: 'tool-result', toolCallId: 'toolu_1', name: 'search_sources', output: '{"count":2}', isError: undefined },
      { type: 'text-delta', delta: '2 件でした。' },
    ])
    const res = parser.result()
    expect(res.sessionId).toBe(SID)
    expect(res.finished).toBe(true)
    expect(res.resultError).toBeUndefined()
    expect(res.parts).toEqual([
      { type: 'text', text: '調べます。' },
      { type: 'tool', toolCallId: 'toolu_1', name: 'search_sources', input: { query: 'ソムデット' }, state: 'done', output: '{"count":2}', isError: undefined },
      { type: 'text', text: '2 件でした。' },
    ])
  })

  it('does not duplicate text that was already streamed as deltas', () => {
    const { events } = run(FIXTURE)
    expect(events.filter(e => e.type === 'text-delta').map(e => (e as { delta: string }).delta).join('')).toBe('調べます。2 件でした。')
  })

  it('emits text of assistant messages that were not streamed (no partial messages)', () => {
    const { events, parser } = run([
      line({ type: 'assistant', message: { id: 'msg_x', content: [{ type: 'text', text: 'そのまま' }] } }),
    ])
    expect(events).toEqual([{ type: 'text-delta', delta: 'そのまま' }])
    expect(parser.parts).toEqual([{ type: 'text', text: 'そのまま' }])
  })

  it('maps tool errors and truncates long outputs to the same limit as before', () => {
    const big = 'x'.repeat(EVENT_OUTPUT_LIMIT + 50)
    const { events } = run([
      line({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 't1', name: 'mcp__phrapedia__read_source', input: { path: 'a.md' } }] } }),
      line({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't1', is_error: true, content: big }] } }),
    ])
    const res = events.find(e => e.type === 'tool-result') as Extract<ChatStreamEvent, { type: 'tool-result' }>
    expect(res.isError).toBe(true)
    expect(String(res.output)).toContain('…(50 文字省略)')
    expect(String(res.output).length).toBeLessThan(EVENT_OUTPUT_LIMIT + 40)
  })

  it('notes images in tool results', () => {
    expect(flattenToolResult([{ type: 'text', text: 'a' }, { type: 'image', source: {} }])).toEqual({ text: 'a', images: 1 })
    const { events } = run([
      line({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 't1', name: 'mcp__phrapedia__read_source', input: {} }] } }),
      line({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't1', content: [{ type: 'image', source: {} }] }] } }),
    ])
    expect((events[1] as { output: string }).output).toContain('[画像 1 件]')
  })

  it('emits saved after a successful save_entity', () => {
    const { events } = run([
      line({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 's1', name: 'mcp__phrapedia__save_entity', input: { kind: 'monks' } }] } }),
      line({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 's1', content: JSON.stringify({ ok: true, kind: 'monks', slug: 'somdet-to', path: 'data/monks/somdet-to.md' }) }] } }),
    ])
    expect(events.map(e => e.type)).toEqual(['tool-start', 'tool-result', 'saved'])
    expect(events[2]).toEqual({ type: 'saved', kind: 'monks', slug: 'somdet-to', path: 'data/monks/somdet-to.md' })
  })

  it('does not emit saved when save_entity failed', () => {
    const { events } = run([
      line({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 's1', name: 'mcp__phrapedia__save_entity', input: {} }] } }),
      line({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 's1', is_error: true, content: JSON.stringify({ ok: true, kind: 'monks', slug: 'x' }) }] } }),
    ])
    expect(events.map(e => e.type)).toEqual(['tool-start', 'tool-result'])
  })

  it('reports an error result and ignores non-JSON noise and sub-agent traffic', () => {
    const { events, parser } = run([
      'not json at all',
      '',
      line({ type: 'assistant', parent_tool_use_id: 'toolu_parent', message: { id: 'sub', content: [{ type: 'text', text: 'sub-agent' }] } }),
      line({ type: 'result', subtype: 'error_during_execution', is_error: true, result: 'No conversation found with session ID: x', session_id: 'x' }),
    ])
    expect(events).toEqual([])
    expect(parser.result()).toMatchObject({ finished: true, resultSubtype: 'error_during_execution', resultError: 'No conversation found with session ID: x', sessionId: 'x' })
  })

  it('marks tools without a result as interrupted', () => {
    const { parser } = run([
      line({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 't1', name: 'mcp__phrapedia__web_research', input: { query: 'q' } }] } }),
    ])
    parser.markInterrupted()
    expect(parser.parts[0]).toMatchObject({ type: 'tool', state: 'done', isError: true, output: '（中断されました）' })
  })

  it('strips the MCP prefix only from phrapedia tools', () => {
    expect(shortToolName('mcp__phrapedia__get_entity')).toBe('get_entity')
    expect(shortToolName('mcp__other__get_entity')).toBe('mcp__other__get_entity')
  })
})

describe('update_draft', () => {
  const draftLines = (input: unknown) => [
    line({ type: 'assistant', message: { id: 'm', content: [{ type: 'tool_use', id: 'd1', name: 'mcp__phrapedia__update_draft', input }] } }),
    line({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'd1', content: 'draft updated (not saved yet)' }] } }),
  ]

  it('turns the tool_use into a draft event and hides the tool call from the UI', () => {
    const { events, parser } = run(draftLines({ markdown: '---\nkind: monks\n---\n# 本文', summary: '  見出しを追加  ' }))
    expect(events).toEqual([{ type: 'draft', markdown: '---\nkind: monks\n---\n# 本文', summary: '見出しを追加' }])
    expect(parser.parts).toEqual([{ type: 'draft', markdown: '---\nkind: monks\n---\n# 本文', summary: '見出しを追加' }])
  })

  it('omits an empty summary and ignores an empty markdown', () => {
    expect(run(draftLines({ markdown: '# a' })).events).toEqual([{ type: 'draft', markdown: '# a', summary: undefined }])
    expect(run(draftLines({ markdown: '   ' })).events).toEqual([])
  })

  it('emits one draft per distinct tool_use id', () => {
    const l = [...draftLines({ markdown: '# a' }), ...draftLines({ markdown: '# a' })]
    expect(run(l).events).toHaveLength(1)
  })
})
