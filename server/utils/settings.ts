import { DEFAULT_SETTINGS } from '../../shared/types/settings'
import type { AppSettings } from '../../shared/types/settings'

const SETTINGS_KEY = 'app'

function deepMerge<T extends Record<string, any>>(base: T, patch: Partial<T> | undefined | null): T {
  if (!patch || typeof patch !== 'object') return { ...base }
  const out: Record<string, any> = { ...base }
  for (const [k, v] of Object.entries(patch)) {
    if (v === undefined) continue
    const b = (base as Record<string, any>)[k]
    if (b && typeof b === 'object' && !Array.isArray(b) && v && typeof v === 'object' && !Array.isArray(v)) {
      out[k] = deepMerge(b, v as Record<string, any>)
    } else {
      out[k] = v
    }
  }
  return out as T
}

/** Defaults = DEFAULT_SETTINGS overridden by runtimeConfig (env), then by the user settings stored in PGlite. */
function envDefaults(): AppSettings {
  const cfg = useRuntimeConfig()
  return deepMerge(DEFAULT_SETTINGS, {
    sourcesDir: cfg.sourcesDir || DEFAULT_SETTINGS.sourcesDir,
    dataDir: cfg.dataDir || DEFAULT_SETTINGS.dataDir,
    ai: { anthropicModel: cfg.anthropicModel || DEFAULT_SETTINGS.ai.anthropicModel },
    gemini: { command: cfg.geminiCommand || DEFAULT_SETTINGS.gemini.command },
  } as Partial<AppSettings>)
}

export async function getSettings(): Promise<AppSettings> {
  const db = await useDb()
  const res = await db.query<{ value: Partial<AppSettings> }>('SELECT value FROM settings WHERE key = $1', [SETTINGS_KEY])
  const stored = res.rows[0]?.value
  return deepMerge(envDefaults(), stored)
}

export async function updateSettings(patch: Partial<AppSettings>): Promise<AppSettings> {
  const db = await useDb()
  const current = await getSettings()
  const next = deepMerge(current, patch)
  await db.query(
    `INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, now())
     ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()`,
    [SETTINGS_KEY, JSON.stringify(next)],
  )
  return next
}

/** Absolute path of the primary-sources directory. */
export async function sourcesDir(): Promise<string> {
  const s = await getSettings()
  return resolveFromRoot(s.sourcesDir)
}

/** Absolute path of the records directory. */
export async function dataDir(): Promise<string> {
  const s = await getSettings()
  return resolveFromRoot(s.dataDir)
}
