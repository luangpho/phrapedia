import type { AppSettings, SettingsView } from '#shared/types'

/** Strip the derived SettingsView extras so only AppSettings is sent back with PUT. */
export function toAppSettings(v: AppSettings | SettingsView): AppSettings {
  return {
    sourcesDir: v.sourcesDir,
    dataDir: v.dataDir,
    ai: { ...v.ai, args: [...(v.ai.args ?? [])] },
    gemini: { ...v.gemini, args: [...(v.gemini.args ?? [])] },
    git: { ...v.git },
  }
}

export function useSettings() {
  const fetched = useAsyncData<SettingsView>('settings', () => api.getSettings())

  async function save(settings: AppSettings) {
    const res = await api.updateSettings(toAppSettings(settings))
    await fetched.refresh()
    return res
  }

  async function reindex() {
    return await api.reindexSources()
  }

  /** Resolves once the initial GET has finished (await it in setup for SSR-consistent forms). */
  const ready = fetched.then(() => undefined)

  return { ...fetched, ready, save, reindex }
}
