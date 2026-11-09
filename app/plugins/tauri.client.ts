/**
 * Desktop (Tauri) only: read the app state (`app_info`). A failure never blocks the UI: the error is shown by
 * <DesktopBanner> and the settings page. In a plain browser nothing happens here; the API layer reports
 * "デスクトップアプリ専用です" per call.
 */
export default defineNuxtPlugin(async () => {
  if (!isTauriRuntime()) return
  const { reload, info } = useDesktop()
  try {
    await reload()
  } catch (err) {
    info.value = {
      version: '',
      settingsWarning: null,
      datasetPath: '',
      exportPath: '',
      datasetCheck: { ok: true, error: null },
      exportCheck: { ok: true, error: null },
      stateDir: '',
      exePath: '',
      configDir: '',
      dataDir: '',
      error: `アプリの状態を取得できません: ${errorMessage(err)}`,
    }
  }
  // fire and forget: never delays startup. Only decides whether the header update icon shows and runs a silent check
  // (no dialog); updating itself starts from the icon.
  void useUpdater().init()
})
