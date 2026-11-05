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

/** Resolve a user-provided directory: absolute stays, relative is taken from the project root. */
export function resolveFromRoot(p: string): string {
  if (!p) return projectRoot()
  return isAbsolute(p) ? p : resolve(projectRoot(), p)
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
