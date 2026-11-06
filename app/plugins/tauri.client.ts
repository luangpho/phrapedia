/**
 * Desktop (Tauri) only: wait for the Node sidecar, then route every /api and /mcp request to it.
 * The WebView origin (tauri://localhost) differs from the sidecar (http://127.0.0.1:<port>), so relative
 * URLs are rewritten at the fetch level; this covers $fetch, useFetch and the chat SSE stream alike.
 * A failed sidecar never blocks the UI: the error is shown by <DesktopBanner> and the settings page.
 */
export default defineNuxtPlugin(async () => {
  if (!isTauriRuntime()) return
  const { reload, info } = useDesktop()
  try {
    await reload()
  } catch (err) {
    info.value = {
      phase: 'error',
      baseUrl: null,
      error: `サーバー状態を取得できません: ${err instanceof Error ? err.message : String(err)}`,
      settingsWarning: null,
      datasetCheck: { ok: true, error: null },
      exportCheck: { ok: true, error: null },
    }
  }
  const nativeFetch = window.fetch.bind(window)
  window.fetch = (input: RequestInfo | URL, init?: RequestInit) =>
    nativeFetch(typeof input === 'string' ? apiUrl(input) : input, init)
})
