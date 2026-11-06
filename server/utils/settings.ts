import { relative, resolve } from 'node:path'
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
    gemini: {
      engine: (cfg.geminiEngine === 'gemini-cli' ? 'gemini-cli' : cfg.geminiEngine === 'antigravity' ? 'antigravity' : DEFAULT_SETTINGS.gemini.engine),
      command: cfg.geminiCommand || DEFAULT_SETTINGS.gemini.command,
    },
  } as Partial<AppSettings>)
}

export async function getSettings(): Promise<AppSettings> {
  const db = await useDb()
  const res = await db.query<{ value: Partial<AppSettings> }>('SELECT value FROM settings WHERE key = $1', [SETTINGS_KEY])
  const stored = res.rows[0]?.value
  return deepMerge(envDefaults(), stored)
}

/**
 * Persist a settings patch. Only the user's own overrides are stored (merged with the previously stored
 * overrides), never the env-derived defaults, so changing NUXT_* variables later still takes effect.
 */
export async function updateSettings(patch: Partial<AppSettings>): Promise<AppSettings> {
  const db = await useDb()
  const res = await db.query<{ value: Partial<AppSettings> }>('SELECT value FROM settings WHERE key = $1', [SETTINGS_KEY])
  const stored = (res.rows[0]?.value ?? {}) as Partial<AppSettings>
  const overrides = deepMerge(stored as AppSettings, patch)
  await db.query(
    `INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, now())
     ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()`,
    [SETTINGS_KEY, JSON.stringify(overrides)],
  )
  return deepMerge(envDefaults(), overrides)
}

/** Git ref / remote names that are safe to pass to git on the command line (no option injection). */
export const GIT_NAME_RE = /^(?!-)(?!.*\.\.)(?!.*[\s~^:?*\[\\])(?!.*\/$)(?!.*\.lock$)[\x21-\x7E\u0080-\uFFFF]{1,200}$/

export function isSafeGitName(name: string): boolean {
  return name === '' || GIT_NAME_RE.test(name)
}

/** Absolute path of the primary-sources directory. */
export async function sourcesDir(): Promise<string> {
  assertDatasetUsable()
  const s = await getSettings()
  return resolveFromRoot(s.sourcesDir)
}

/** Absolute path of the records directory to read from (inside the dataset path). */
export async function dataDir(): Promise<string> {
  assertDatasetUsable()
  const s = await getSettings()
  return resolveFromRoot(s.dataDir)
}

/**
 * Absolute path of the records directory to write to (inside the export path), mirroring dataDir()'s
 * location relative to the dataset root. Equals dataDir() when no separate export path is configured.
 */
export async function exportDataDir(): Promise<string> {
  const read = await dataDir()
  const exportBase = exportRoot()
  if (exportBase === datasetRoot()) return read
  assertExportUsable()
  const base = datasetRoot()
  const rel = isInside(base, read) ? relative(base, read) : 'data'
  return resolve(exportBase, rel || 'data')
}
