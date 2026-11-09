import { isTauriRuntime } from '../composables/useDesktop'

export interface ConfirmDialogOptions {
  title?: string
  okLabel?: string
  cancelLabel?: string
}

/**
 * Confirmation dialog. Returns true when the user accepts.
 * Tauri: plugin-dialog `ask` (window.confirm is a no-op Cancel in WKWebView because wry has no JS panel).
 * Browser (Render / nuxt dev): window.confirm.
 */
export async function confirmDialog(message: string, options: ConfirmDialogOptions = {}): Promise<boolean> {
  if (isTauriRuntime()) {
    const { ask } = await import('@tauri-apps/plugin-dialog')
    return ask(message, {
      title: options.title ?? 'Phrapedia',
      kind: 'warning',
      okLabel: options.okLabel,
      cancelLabel: options.cancelLabel,
    })
  }
  return window.confirm(message)
}
