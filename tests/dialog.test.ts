import { beforeEach, describe, expect, it, vi } from 'vitest'

const isTauriRuntime = vi.fn()
const ask = vi.fn()
vi.mock('../app/utils/api', () => ({ isTauriRuntime: () => isTauriRuntime() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: (...a: unknown[]) => ask(...a) }))

const { confirmDialog } = await import('../app/utils/dialog')

describe('confirmDialog', () => {
  const confirm = vi.fn()
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('window', { confirm })
  })

  it('Tauri: plugin-dialog ask を呼び、結果を返す', async () => {
    isTauriRuntime.mockReturnValue(true)
    ask.mockResolvedValue(true)
    await expect(confirmDialog('破棄しますか？', { title: 'T', okLabel: 'はい', cancelLabel: 'いいえ' })).resolves.toBe(true)
    expect(ask).toHaveBeenCalledWith('破棄しますか？', { title: 'T', kind: 'warning', okLabel: 'はい', cancelLabel: 'いいえ' })
    expect(confirm).not.toHaveBeenCalled()
  })

  it('Tauri: ask が false なら false', async () => {
    isTauriRuntime.mockReturnValue(true)
    ask.mockResolvedValue(false)
    await expect(confirmDialog('x')).resolves.toBe(false)
  })

  it('Tauri: ask が reject したら false（編集を破棄しない）', async () => {
    isTauriRuntime.mockReturnValue(true)
    ask.mockRejectedValue(new Error('denied'))
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    await expect(confirmDialog('x')).resolves.toBe(false)
    expect(confirm).not.toHaveBeenCalled()
    warn.mockRestore()
  })

  it('ブラウザ: window.confirm を呼ぶ', async () => {
    isTauriRuntime.mockReturnValue(false)
    confirm.mockReturnValue(true)
    await expect(confirmDialog('x')).resolves.toBe(true)
    expect(confirm).toHaveBeenCalledWith('x')
    expect(ask).not.toHaveBeenCalled()
  })
})
