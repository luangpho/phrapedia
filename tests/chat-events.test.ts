import { describe, expect, it } from 'vitest'
import type { ChatMessage } from '../shared/types/chat'
import { applyChatStreamEvent, chatToUiMessages } from '../app/composables/useChat'
import { encodeSourcePath, rewriteSourceHref, sourcePathFromHref } from '../app/utils/sources'

const blank = (): ChatMessage => ({ id: 'a', role: 'assistant', parts: [], createdAt: '2026-01-01T00:00:00Z' })

describe('applyChatStreamEvent (ChatStreamEvent contract is unchanged)', () => {
  it('appends text deltas to the last text part', () => {
    const m = blank()
    applyChatStreamEvent(m, { type: 'text-delta', delta: 'ab' })
    applyChatStreamEvent(m, { type: 'text-delta', delta: 'c' })
    expect(m.parts).toEqual([{ type: 'text', text: 'abc' }])
  })

  it('tracks tool start / result', () => {
    const m = blank()
    applyChatStreamEvent(m, { type: 'tool-start', toolCallId: 't', name: 'search_sources', input: { q: 'x' } })
    expect(m.parts[0]).toMatchObject({ type: 'tool', state: 'running' })
    applyChatStreamEvent(m, { type: 'tool-result', toolCallId: 't', name: 'search_sources', output: 'r' })
    expect(m.parts[0]).toMatchObject({ state: 'done', output: 'r' })
  })

  it('returns draft / saved / final / error for the caller', () => {
    const m = blank()
    expect(applyChatStreamEvent(m, { type: 'draft', markdown: '# x', summary: 's' }).draft).toEqual({ markdown: '# x', summary: 's' })
    expect(applyChatStreamEvent(m, { type: 'saved', kind: 'monks', slug: 'x', path: 'data/monks/x.md' }).saved).toMatchObject({ slug: 'x' })
    expect(applyChatStreamEvent(m, { type: 'error', message: 'e' }).error).toBe('e')
    const done = blank()
    expect(applyChatStreamEvent(m, { type: 'done', message: done }).final).toBe(done)
  })

  it('chatToUiMessages marks only the last streaming text part', () => {
    const m: ChatMessage = { ...blank(), parts: [{ type: 'text', text: 'a' }] }
    expect(chatToUiMessages([m], { streamingId: 'a' })[0]!.parts[0]).toMatchObject({ type: 'text', state: 'streaming' })
    expect(chatToUiMessages([m])[0]!.parts[0]).toMatchObject({ state: 'done' })
  })
})

describe('source marker hrefs (no HTTP URL any more)', () => {
  it('rewrites relative paths to marker hrefs and back', () => {
    expect(rewriteSourceHref('sources/images/ก.jpg')).toBe(`#phra-src/${encodeSourcePath('images/ก.jpg')}`)
    expect(sourcePathFromHref(rewriteSourceHref('sources/images/ก.jpg'))).toBe('images/ก.jpg')
    expect(sourcePathFromHref(rewriteSourceHref('./a b.png'))).toBe('a b.png')
  })

  it('leaves external / absolute / anchor hrefs alone', () => {
    for (const h of ['https://x.test/a.png', '/abs.png', '#top', 'data:image/png;base64,AA']) expect(rewriteSourceHref(h)).toBe(h)
    expect(sourcePathFromHref('#top')).toBeNull()
    expect(sourcePathFromHref(null)).toBeNull()
  })
})
