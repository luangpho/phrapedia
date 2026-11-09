/**
 * Desktop (Tauri) only: check for an update once at startup and, if the user agrees,
 * download, install and relaunch. Any failure (updater not configured, offline, ...) is ignored silently.
 */
let checked = false

export function useUpdater() {
  async function checkOnce(): Promise<void> {
    if (checked || !isTauriRuntime()) return
    checked = true
    try {
      const { check } = await import('@tauri-apps/plugin-updater')
      const update = await check()
      if (!update) return
      if (!window.confirm(`更新があります（${update.version}）。今すぐ更新して再起動しますか`)) return
      await update.downloadAndInstall()
      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (err) {
      console.info('[updater] skipped:', err instanceof Error ? err.message : String(err))
    }
  }

  return { checkOnce }
}
