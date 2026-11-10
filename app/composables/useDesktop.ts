import type { AppInfo, PathSettingsView, SavePathsOutcome } from '~/utils/api'

/** `app_info` plus a frontend-side error (app_info itself failed). */
export type DesktopInfo = AppInfo & { error: string | null }

/** Desktop (Tauri) integration. `isDesktop` is false in the browser, so callers can hide desktop-only UI. */
export function useDesktop() {
  const info = useState<DesktopInfo | null>('desktop-info', () => null)
  const isDesktop = computed(() => info.value !== null)

  /** Read the app state (instant; there is no server to wait for). */
  async function reload(): Promise<DesktopInfo> {
    const next: DesktopInfo = { ...(await api.appInfo()), error: null }
    info.value = next
    return next
  }

  async function getPaths(): Promise<PathSettingsView> {
    return api.getPathSettings()
  }

  /** Validated in Rust; persisted and applied immediately only when both paths are valid. */
  async function savePaths(datasetPath: string, exportPath: string): Promise<SavePathsOutcome> {
    const out = await api.savePathSettings(datasetPath, exportPath)
    if (out.ok) {
      clearSourceBlobCache()
      await reload()
      await refreshNuxtData()
    }
    return out
  }

  /** Native folder picker; null when cancelled. */
  async function pickFolder(defaultPath?: string): Promise<string | null> {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ directory: true, multiple: false, defaultPath: defaultPath || undefined })
    return typeof picked === 'string' ? picked : null
  }

  return { info, isDesktop, reload, getPaths, savePaths, pickFolder }
}
