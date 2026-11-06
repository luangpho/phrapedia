export interface PathCheckView { ok: boolean; error: string | null }

export interface DesktopInfo {
  phase: 'starting' | 'ready' | 'error'
  baseUrl: string | null
  error: string | null
  settingsWarning: string | null
  datasetCheck: PathCheckView
  exportCheck: PathCheckView
}

export interface DesktopPathSettings {
  datasetPath: string
  /** Empty = same as the dataset path */
  exportPath: string
  effectiveExport: string
  datasetCheck: PathCheckView
  exportCheck: PathCheckView
  settingsFile: string
}

export interface SavePathsOutcome {
  ok: boolean
  datasetError: string | null
  exportError: string | null
  restartError: string | null
  settings: DesktopPathSettings
}

/** True inside the Tauri WebView. */
export function isTauriRuntime(): boolean {
  return import.meta.client && typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

/** Desktop (Tauri) integration. `isDesktop` is false in the browser / CLI, so callers can hide desktop-only UI. */
export function useDesktop() {
  const info = useState<DesktopInfo | null>('desktop-info', () => null)
  const isDesktop = computed(() => info.value !== null)

  /** Wait for the sidecar, store its state and point the API base at it. */
  async function reload(): Promise<DesktopInfo> {
    const next = await call<DesktopInfo>('sidecar_info')
    setApiBase(next.baseUrl)
    info.value = next
    return next
  }

  async function getPaths(): Promise<DesktopPathSettings> {
    return call<DesktopPathSettings>('get_path_settings')
  }

  /** Validates in Rust; persists and restarts the server only when both paths are valid. */
  async function savePaths(datasetPath: string, exportPath: string): Promise<SavePathsOutcome> {
    const out = await call<SavePathsOutcome>('save_path_settings', { datasetPath, exportPath })
    if (out.ok) {
      await reload()
      await refreshNuxtData()
    }
    return out
  }

  async function restart(): Promise<void> {
    info.value = info.value ? { ...info.value, phase: 'starting', error: null } : info.value
    await call<void>('restart_sidecar')
    await reload()
    await refreshNuxtData()
  }

  /** Native folder picker; null when cancelled. */
  async function pickFolder(defaultPath?: string): Promise<string | null> {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ directory: true, multiple: false, defaultPath: defaultPath || undefined })
    return typeof picked === 'string' ? picked : null
  }

  return { info, isDesktop, reload, getPaths, savePaths, restart, pickFolder }
}
