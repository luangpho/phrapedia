import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const invoke = vi.fn()
class FakeChannel<T> {
  static last: FakeChannel<unknown> | null = null
  onmessage: ((ev: T) => void) | undefined
  constructor() { FakeChannel.last = this as FakeChannel<unknown> }
}
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a), Channel: FakeChannel }))

const mod = await import('../app/utils/api')
const { api, ApiError, appErrorMessage, call, chatSend, DESKTOP_ONLY_MESSAGE, isAppError, isTauriRuntime, mimeFromPath, sourceBlobUrl, toApiError } = mod

const g = globalThis as Record<string, unknown>

function asDesktop() { g.window = { __TAURI_INTERNALS__: {} } }
function asBrowser() { delete g.window }

beforeEach(() => {
  invoke.mockReset()
  FakeChannel.last = null
  asDesktop()
})
afterEach(() => asBrowser())

describe('import / runtime guard', () => {
  it('importing the module did not throw and the browser is detected', () => {
    asBrowser()
    expect(isTauriRuntime()).toBe(false)
    asDesktop()
    expect(isTauriRuntime()).toBe(true)
  })

  it('rejects with a Japanese desktop-only message outside Tauri (no invoke)', async () => {
    asBrowser()
    const err = await api.listEntities('monks').catch(e => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err.code).toBe('desktop_only')
    expect(err.message).toBe(DESKTOP_ONLY_MESSAGE)
    expect(err.message).toContain('デスクトップアプリ専用')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('chatSend outside Tauri rejects instead of throwing synchronously', async () => {
    asBrowser()
    const h = chatSend({ turnId: 't', kind: 'monks', message: 'x', draft: '' }, () => {})
    await expect(h.done).rejects.toMatchObject({ code: 'desktop_only' })
  })
})

describe('command names and arguments (docs/NATIVE_API.md)', () => {
  it.each([
    ['appInfo', () => api.appInfo(), 'app_info', undefined],
    ['updaterEnabled', () => api.updaterEnabled(), 'updater_enabled', undefined],
    ['getPathSettings', () => api.getPathSettings(), 'get_path_settings', undefined],
    ['savePathSettings', () => api.savePathSettings('/d', '/e'), 'save_path_settings', { datasetPath: '/d', exportPath: '/e' }],
    ['getSettings', () => api.getSettings(), 'get_settings', undefined],
    ['updateSettings', () => api.updateSettings({ git: { autoCommit: true } }), 'update_settings', { patch: { git: { autoCommit: true } } }],
    ['reindexSources', () => api.reindexSources(), 'reindex_sources', undefined],
    ['listEntities', () => api.listEntities('temples'), 'list_entities', { kind: 'temples' }],
    ['entityTemplate', () => api.entityTemplate('monks', 'หลวงปู่'), 'entity_template', { kind: 'monks', name: 'หลวงปู่' }],
    ['entityTemplate (no name)', () => api.entityTemplate('monks', ''), 'entity_template', { kind: 'monks', name: undefined }],
    ['createEntity', () => api.createEntity('monks', 'raw', 'msg'), 'create_entity', { kind: 'monks', raw: 'raw', message: 'msg' }],
    ['getEntity', () => api.getEntity('monks', 'x'), 'get_entity', { kind: 'monks', slug: 'x' }],
    ['updateEntity', () => api.updateEntity('monks', 'x', 'raw'), 'update_entity', { kind: 'monks', slug: 'x', raw: 'raw', message: undefined }],
    ['deleteEntity', () => api.deleteEntity('monks', 'x'), 'delete_entity', { kind: 'monks', slug: 'x' }],
    ['listSources', () => api.listSources('images'), 'list_sources', { dir: 'images' }],
    ['searchSources', () => api.searchSources('abc', 30), 'search_sources', { q: 'abc', limit: 30 }],
    ['readSource', () => api.readSource('a/b.md', 200000), 'read_source', { path: 'a/b.md', maxChars: 200000 }],
    ['webResearch', () => api.webResearch({ query: 'q', noCache: true }), 'web_research', { query: 'q', noCache: true }],
    ['syncStatus', () => api.syncStatus(), 'sync_status', undefined],
    ['syncRun commit', () => api.syncRun('commit', 'm'), 'commit_data', { message: 'm' }],
    ['syncRun sync', () => api.syncRun('sync', undefined), 'sync_data', { message: undefined }],
    ['syncRun pull', () => api.syncRun('pull'), 'pull_data', undefined],
    ['syncRun push', () => api.syncRun('push'), 'push_data', undefined],
    ['listChatSessions', () => api.listChatSessions('monks', 'x'), 'list_chat_sessions', { kind: 'monks', slug: 'x' }],
    ['listChatSessions (draft)', () => api.listChatSessions('monks', undefined), 'list_chat_sessions', { kind: 'monks', slug: null }],
    ['getChatSession', () => api.getChatSession('s1'), 'get_chat_session', { id: 's1' }],
    ['deleteChatSession', () => api.deleteChatSession('s1'), 'delete_chat_session', { id: 's1' }],
    ['patchChatSession', () => api.patchChatSession('s1', { slug: 'x' }), 'patch_chat_session', { id: 's1', patch: { slug: 'x' } }],
    ['deleteChatMessages', () => api.deleteChatMessages('s1', ['a', 'b']), 'delete_chat_messages', { id: 's1', ids: ['a', 'b'] }],
    ['chatAbort', () => api.chatAbort('t1'), 'chat_abort', { turnId: 't1' }],
  ])('%s', async (_name, run, cmd, args) => {
    invoke.mockResolvedValue('ok')
    expect(await run()).toBe('ok')
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith(cmd, args)
  })
})

describe('error conversion', () => {
  it('turns an AppError JSON into an ApiError keeping code / message / detail', async () => {
    invoke.mockRejectedValue({ code: 'path_unusable', message: 'データセットパスが使えません: /x', detail: 'ENOENT' })
    const err = await api.listEntities('monks').catch(e => e)
    expect(err).toBeInstanceOf(ApiError)
    expect(err).toMatchObject({ code: 'path_unusable', message: 'データセットパスが使えません: /x', detail: 'ENOENT', statusCode: 409 })
  })

  it('maps not_found to statusCode 404 (the record page relies on it)', async () => {
    invoke.mockRejectedValue({ code: 'not_found', message: '見つかりません' })
    expect(await api.getEntity('monks', 'x').catch(e => e)).toMatchObject({ code: 'not_found', statusCode: 404 })
  })

  it('wraps strings and Errors as internal', () => {
    expect(toApiError('boom')).toMatchObject({ code: 'internal', message: 'boom' })
    expect(toApiError(new Error('bad'))).toMatchObject({ code: 'internal', message: 'bad' })
    expect(toApiError(undefined).message).not.toBe('')
  })

  it('isAppError / appErrorMessage', () => {
    expect(isAppError({ code: 'io', message: 'x' })).toBe(true)
    expect(isAppError({ code: 1, message: 'x' })).toBe(false)
    expect(isAppError('x')).toBe(false)
    expect(appErrorMessage({ code: 'io', message: '失敗' })).toBe('失敗')
    expect(appErrorMessage(new Error('e'))).toBe('e')
    expect(appErrorMessage(null, 'fb')).toBe('fb')
  })

  it('call() passes arguments through unchanged', async () => {
    invoke.mockResolvedValue(1)
    await call('x', { a: 1 })
    expect(invoke).toHaveBeenCalledWith('x', { a: 1 })
  })
})

describe('chatSend (Channel)', () => {
  it('passes request + Channel to chat_send and relays events in order until resolve', async () => {
    let finish!: () => void
    invoke.mockImplementation(() => new Promise<void>((r) => { finish = r }))
    const got: string[] = []
    const request = { turnId: 't1', kind: 'monks' as const, slug: null, message: 'hi', draft: '' }
    const h = chatSend(request, ev => got.push(ev.type))
    await vi.waitFor(() => expect(invoke).toHaveBeenCalled())
    const [cmd, args] = invoke.mock.calls[0]!
    expect(cmd).toBe('chat_send')
    expect(args.request).toEqual(request)
    expect(args.onEvent).toBe(FakeChannel.last)
    FakeChannel.last!.onmessage!({ type: 'text-delta', delta: 'a' })
    FakeChannel.last!.onmessage!({ type: 'error', message: 'x' })
    finish()
    await h.done
    expect(got).toEqual(['text-delta', 'error'])
  })

  it('detach() ignores late events', async () => {
    let finish!: () => void
    invoke.mockImplementation(() => new Promise<void>((r) => { finish = r }))
    const got: string[] = []
    const h = chatSend({ turnId: 't', kind: 'monks', message: 'x', draft: '' }, ev => got.push(ev.type))
    await vi.waitFor(() => expect(FakeChannel.last).not.toBeNull())
    h.detach()
    FakeChannel.last!.onmessage!({ type: 'text-delta', delta: 'late' })
    finish()
    await h.done
    expect(got).toEqual([])
  })

  it('rejects with an ApiError when chat_send rejects (invalid input)', async () => {
    invoke.mockRejectedValue({ code: 'invalid_argument', message: 'メッセージが空です' })
    const h = chatSend({ turnId: 't', kind: 'monks', message: '', draft: '' }, () => {})
    await expect(h.done).rejects.toMatchObject({ code: 'invalid_argument', statusCode: 400 })
  })
})

describe('source bytes', () => {
  it('mimeFromPath', () => {
    expect(mimeFromPath('a/b.PNG')).toBe('image/png')
    expect(mimeFromPath('x.jpg')).toBe('image/jpeg')
    expect(mimeFromPath('x.pdf')).toBe('application/pdf')
    expect(mimeFromPath('x.bin')).toBe('application/octet-stream')
  })

  it('sourceBlobUrl reads once per path and builds a typed Blob URL', async () => {
    const created: Blob[] = []
    const url = vi.spyOn(URL, 'createObjectURL').mockImplementation((b) => { created.push(b as Blob); return `blob:test/${created.length}` })
    invoke.mockResolvedValue(new Uint8Array([1, 2, 3]).buffer)
    const a = await sourceBlobUrl('images/cache-test.png')
    const b = await sourceBlobUrl('images/cache-test.png')
    expect(a).toBe(b)
    expect(invoke).toHaveBeenCalledTimes(1)
    expect(invoke).toHaveBeenCalledWith('read_source_bytes', { path: 'images/cache-test.png' })
    expect(created[0]!.type).toBe('image/png')
    expect(created[0]!.size).toBe(3)
    url.mockRestore()
  })

  it('does not cache a failed read', async () => {
    invoke.mockRejectedValueOnce({ code: 'not_found', message: 'x' })
    await expect(sourceBlobUrl('images/missing.png')).rejects.toMatchObject({ code: 'not_found' })
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:ok')
    invoke.mockResolvedValueOnce(new ArrayBuffer(1))
    await expect(sourceBlobUrl('images/missing.png')).resolves.toBe('blob:ok')
  })
})
