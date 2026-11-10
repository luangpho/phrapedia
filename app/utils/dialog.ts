import { isTauriRuntime } from './api'

export interface ConfirmDialogOptions {
  title?: string
  okLabel?: string
  cancelLabel?: string
}

/**
 * Confirmation dialog. Returns true when the user accepts.
 * Tauri: plugin-dialog `ask` (window.confirm is a no-op Cancel in WKWebView because wry has no JS panel).
 * Browser (nuxt dev): window.confirm.
 * If ask rejects (e.g. missing permission) it resolves false: keep the user's edits rather than silently discard them
 * (window.confirm is no fallback here, it is a no-op Cancel in WKWebView).
 */
export async function confirmDialog(message: string, options: ConfirmDialogOptions = {}): Promise<boolean> {
  if (isTauriRuntime()) {
    try {
      const { ask } = await import('@tauri-apps/plugin-dialog')
      return await ask(message, {
        title: options.title ?? 'Phrapedia',
        kind: 'warning',
        okLabel: options.okLabel,
        cancelLabel: options.cancelLabel,
      })
    } catch (err) {
      console.warn('[dialog] ask failed:', err instanceof Error ? err.message : String(err))
      return false
    }
  }
  return window.confirm(message)
}
