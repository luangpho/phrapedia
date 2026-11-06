import { resolve, isAbsolute, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Project root = directory that contains package.json / data / sources.
 * In dev and in `node .output/server/index.mjs` started from the project root this is process.cwd().
 * Override with PHRAPEDIA_ROOT when running elsewhere.
 */
export function projectRoot(): string {
  const env = process.env.PHRAPEDIA_ROOT
  if (env) return resolve(env)
  return process.cwd()
}

/** Desktop (Tauri) mode = the two-path settings are in use. CLI mode (env unset) keeps the legacy layout. */
export function isDesktopMode(): boolean {
  return !!process.env.PHRAPEDIA_DATASET_PATH
}

/**
 * Dataset root. Desktop (Tauri) app: PHRAPEDIA_DATASET_PATH = the data/ equivalent directory
 * (<kind>/*.md and sources/ directly inside). CLI: unset, so it equals projectRoot() (data/ lives under it).
 */
export function datasetRoot(): string {
  const env = process.env.PHRAPEDIA_DATASET_PATH
  return env ? resolve(env) : projectRoot()
}

/**
 * Export root = directory records are written to (data/ equivalent); also the git target for sync.
 * Desktop (Tauri) app: PHRAPEDIA_EXPORT_PATH. Unset or empty = same as datasetRoot().
 */
export function exportRoot(): string {
  const env = process.env.PHRAPEDIA_EXPORT_PATH
  return env ? resolve(env) : datasetRoot()
}

/** Resolve a user-provided directory: absolute stays, relative is taken from the dataset root. */
export function resolveFromRoot(p: string): string {
  if (!p) return datasetRoot()
  return isAbsolute(p) ? p : resolve(datasetRoot(), p)
}

/** True when `child` is inside `parent` (or equal). Both must be absolute. */
export function isInside(parent: string, child: string): boolean {
  const rel = relative(parent, child)
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel))
}

/** Join a relative, user-supplied path under a base dir, rejecting traversal. */
export function safeJoin(baseDir: string, relPath: string): string {
  const normalised = (relPath ?? '').replace(/\\/g, '/').replace(/^\/+/, '')
  const full = resolve(baseDir, normalised)
  if (!isInside(baseDir, full)) {
    throw createError({ statusCode: 400, statusMessage: 'Invalid path', message: `Path escapes base directory: ${relPath}` })
  }
  return full
}

export function toPosix(p: string): string {
  return sep === '/' ? p : p.split(sep).join('/')
}

export function stateDir(): string {
  const cfg = useRuntimeConfig()
  return resolveFromRoot(cfg.stateDir || '.phrapedia')
}

// Keep fileURLToPath referenced for environments that need __dirname-like behaviour
export const _fileURLToPath = fileURLToPath
