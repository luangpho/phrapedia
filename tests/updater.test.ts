import { beforeEach, describe, expect, it, vi } from 'vitest'

const isTauriRuntime = vi.fn()
const check = vi.fn()
const relaunch = vi.fn()
const ask = vi.fn()
const invoke = vi.fn()
const getVersion = vi.fn()
const pluginImported = vi.fn()

vi.mock('../app/composables/useDesktop', () => ({ isTauriRuntime: () => isTauriRuntime() }))
vi.mock('@tauri-apps/plugin-updater', () => { pluginImported('updater'); return { check: (...a: unknown[]) => check(...a) } })
vi.mock('@tauri-apps/plugin-process', () => { pluginImported('process'); return { relaunch: () => relaunch() } })
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: (...a: unknown[]) => ask(...a) }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: () => getVersion() }))

const { useUpdater, __resetUpdaterForTests, isPermissionError, BREW_UPGRADE_COMMAND, CHECK_INTERVAL_MS } = await import('../app/composables/useUpdater')

function fakeUpdate(download: (cb: (ev: unknown) => void) => Promise<void> = async () => {}) {
  return { version: '0.2.0', body: '- 新機能 A', downloadAndInstall: vi.fn(download) }
}

describe('useUpdater', () => {
  const notify = vi.fn()
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useRealTimers()
    __resetUpdaterForTests()
    isTauriRuntime.mockReturnValue(true)
    invoke.mockResolvedValue(true)
    getVersion.mockResolvedValue('0.1.2')
    check.mockResolvedValue(null)
    ask.mockResolvedValue(true)
    relaunch.mockResolvedValue(undefined)
    vi.spyOn(console, 'info').mockImplementation(() => {})
    vi.spyOn(console, 'warn').mockImplementation(() => {})
  })

  it('非 Tauri: init は何もせず、plugin も import しない', async () => {
    isTauriRuntime.mockReturnValue(false)
    const u = useUpdater()
    await u.init()
    await u.handleClick(notify)
    expect(u.enabled.value).toBe(false)
    expect(invoke).not.toHaveBeenCalled()
    expect(pluginImported).not.toHaveBeenCalled()
    expect(check).not.toHaveBeenCalled()
    expect(notify).not.toHaveBeenCalled()
  })

  it('updater 無効（updater_enabled=false）: 表示されず check もしない', async () => {
    invoke.mockResolvedValue(false)
    const u = useUpdater()
    await u.init()
    expect(u.enabled.value).toBe(false)
    expect(check).not.toHaveBeenCalled()
    await u.handleClick(notify)
    expect(check).not.toHaveBeenCalled()
  })

  it('updater_enabled が失敗しても壊れない（無効扱い）', async () => {
    invoke.mockRejectedValue(new Error('unknown command'))
    const u = useUpdater()
    await u.init()
    expect(u.enabled.value).toBe(false)
    expect(check).not.toHaveBeenCalled()
  })

  it('起動時: 静かに check し、更新ありなら available（ダイアログは出さない）', async () => {
    check.mockResolvedValue(fakeUpdate())
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    expect(u.state.value).toEqual({ kind: 'available', version: '0.2.0', notes: '- 新機能 A' })
    expect(u.currentVersion.value).toBe('0.1.2')
    expect(ask).not.toHaveBeenCalled()
    expect(notify).not.toHaveBeenCalled()
  })

  it('起動時: 更新なしなら idle のまま、toast もダイアログも無し', async () => {
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(check).toHaveBeenCalledTimes(1))
    await vi.waitFor(() => expect(u.state.value.kind).toBe('idle'))
    expect(ask).not.toHaveBeenCalled()
    expect(notify).not.toHaveBeenCalled()
  })

  it('起動時: check 失敗は静かに idle（toast 無し）', async () => {
    check.mockRejectedValue(new Error('offline'))
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(check).toHaveBeenCalled())
    await vi.waitFor(() => expect(u.state.value.kind).toBe('idle'))
    expect(notify).not.toHaveBeenCalled()
    expect(ask).not.toHaveBeenCalled()
  })

  it('6 時間ごとに再 check する', async () => {
    vi.useFakeTimers()
    const u = useUpdater()
    await u.init()
    await vi.advanceTimersByTimeAsync(0)
    expect(check).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(CHECK_INTERVAL_MS)
    expect(check).toHaveBeenCalledTimes(2)
    expect(CHECK_INTERVAL_MS).toBe(6 * 60 * 60 * 1000)
  })

  it('クリック（available）: 確認に版と release notes を出し、download→relaunch', async () => {
    const update = fakeUpdate(async (cb) => {
      cb({ event: 'Started', data: { contentLength: 200 } })
      cb({ event: 'Progress', data: { chunkLength: 100 } })
      cb({ event: 'Finished' })
    })
    check.mockResolvedValue(update)
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    await u.handleClick(notify)
    expect(ask).toHaveBeenCalledTimes(1)
    const msg = String(ask.mock.calls[0]![0])
    expect(msg).toContain('0.2.0')
    expect(msg).toContain('0.1.2')
    expect(msg).toContain('新機能 A')
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1)
    expect(relaunch).toHaveBeenCalledTimes(1)
    expect(u.state.value.kind).toBe('installing')
  })

  it('ダウンロード進捗 % が downloading に出る', async () => {
    const seen: unknown[] = []
    const u = useUpdater()
    const update = fakeUpdate(async (cb) => {
      cb({ event: 'Started', data: { contentLength: 200 } })
      cb({ event: 'Progress', data: { chunkLength: 50 } })
      seen.push({ ...u.state.value })
      cb({ event: 'Progress', data: { chunkLength: 150 } })
      seen.push({ ...u.state.value })
    })
    check.mockResolvedValue(update)
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    await u.handleClick(notify)
    expect(seen).toEqual([
      { kind: 'downloading', version: '0.2.0', percent: 25 },
      { kind: 'downloading', version: '0.2.0', percent: 100 },
    ])
  })

  it('確認で「あとで」: install せず available のまま', async () => {
    const update = fakeUpdate()
    check.mockResolvedValue(update)
    ask.mockResolvedValue(false)
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    await u.handleClick(notify)
    expect(update.downloadAndInstall).not.toHaveBeenCalled()
    expect(relaunch).not.toHaveBeenCalled()
    expect(u.state.value.kind).toBe('available')
  })

  it('クリック（available でない）: その場で check、更新なしなら「最新です（現在 x.y.z）」toast', async () => {
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('idle'))
    check.mockClear()
    await u.handleClick(notify)
    expect(check).toHaveBeenCalledTimes(1)
    expect(notify).toHaveBeenCalledWith({ title: '最新です（現在 0.1.2）', color: 'success' })
    expect(u.state.value.kind).toBe('upToDate')
    expect(ask).not.toHaveBeenCalled()
  })

  it('クリック（available でない）: その場の check で更新が見つかれば確認へ進む', async () => {
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('idle'))
    const update = fakeUpdate()
    check.mockResolvedValue(update)
    await u.handleClick(notify)
    expect(ask).toHaveBeenCalledTimes(1)
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1)
  })

  it('クリック時の check 失敗は toast（握り潰さない）+ error 状態', async () => {
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('idle'))
    check.mockRejectedValue(new Error('network down'))
    await u.handleClick(notify)
    expect(notify).toHaveBeenCalledWith({ title: '更新の確認に失敗しました', description: 'network down', color: 'error' })
    expect(u.state.value).toEqual({ kind: 'error', message: 'network down' })
  })

  it('download 失敗は toast + error。relaunch しない', async () => {
    const update = fakeUpdate(async () => { throw new Error('signature mismatch') })
    check.mockResolvedValue(update)
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    await u.handleClick(notify)
    expect(notify).toHaveBeenCalledWith({ title: '更新に失敗しました', description: 'signature mismatch', color: 'error' })
    expect(u.state.value).toEqual({ kind: 'error', message: 'signature mismatch' })
    expect(relaunch).not.toHaveBeenCalled()
  })

  it('権限エラーは brew upgrade --cask phrapedia の案内を出す', async () => {
    const update = fakeUpdate(async () => { throw new Error('Permission denied (os error 13)') })
    check.mockResolvedValue(update)
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    await u.handleClick(notify)
    const arg = notify.mock.calls[0]![0]
    expect(arg.title).toBe('権限不足のため更新できませんでした')
    expect(arg.description).toContain('brew upgrade --cask phrapedia')
    expect(arg.description).toContain('os error 13')
    expect(BREW_UPGRADE_COMMAND).toBe('brew upgrade --cask phrapedia')
  })

  it('isPermissionError: 権限系だけ true', () => {
    expect(isPermissionError('Permission denied (os error 13)')).toBe(true)
    expect(isPermissionError('Operation not permitted (os error 1)')).toBe(true)
    expect(isPermissionError('Read-only file system')).toBe(true)
    expect(isPermissionError('signature mismatch')).toBe(false)
    expect(isPermissionError('network down')).toBe(false)
  })

  it('多重クリック: 確認中の再クリックは無視され、download は 1 回', async () => {
    const update = fakeUpdate()
    check.mockResolvedValue(update)
    let resolveAsk!: (v: boolean) => void
    ask.mockReturnValue(new Promise<boolean>((r) => { resolveAsk = r }))
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    const first = u.handleClick(notify)
    await vi.waitFor(() => expect(ask).toHaveBeenCalledTimes(1))
    await u.handleClick(notify)
    await u.handleClick(notify)
    expect(ask).toHaveBeenCalledTimes(1)
    resolveAsk(true)
    await first
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1)
  })

  it('多重実行: downloading 中のクリック・静かな check は何もしない', async () => {
    let finish!: () => void
    const update = fakeUpdate(() => new Promise<void>((r) => { finish = r }))
    check.mockResolvedValue(update)
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    const run = u.handleClick(notify)
    await vi.waitFor(() => expect(u.state.value.kind).toBe('downloading'))
    check.mockClear()
    await u.handleClick(notify)
    await u.silentCheck()
    expect(check).not.toHaveBeenCalled()
    expect(update.downloadAndInstall).toHaveBeenCalledTimes(1)
    finish()
    await run
  })

  it('静かな check の一時失敗で既存の available を落とさない', async () => {
    check.mockResolvedValue(fakeUpdate())
    const u = useUpdater()
    await u.init()
    await vi.waitFor(() => expect(u.state.value.kind).toBe('available'))
    check.mockRejectedValue(new Error('offline'))
    await u.silentCheck()
    expect(u.state.value.kind).toBe('available')
  })
})
