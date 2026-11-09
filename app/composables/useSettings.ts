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
  const fetched = useFetch<SettingsView>('/api/settings', { key: 'settings' })

  async function save(settings: AppSettings) {
    const res = await $fetch<AppSettings | SettingsView>('/api/settings', { method: 'PUT', body: toAppSettings(settings) })
    await fetched.refresh()
    return res
  }

  async function reindex() {
    return await $fetch<{ indexed: number; removed: number; total: number }>('/api/sources/reindex', { method: 'POST' })
  }

  /** Resolves once the initial GET has finished (await it in setup for SSR-consistent forms). */
  const ready = fetched.then(() => undefined)

  return { ...fetched, ready, save, reindex }
}
