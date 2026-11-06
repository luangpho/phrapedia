import { accessSync, constants, statSync } from 'node:fs'

export interface PathCheck {
  ok: boolean
  /** Human readable (Japanese) reason when ok is false */
  error?: string
}

/** Dataset path: must be an existing, readable directory. */
export function checkDatasetPath(p: string): PathCheck {
  return checkDir(p, { write: false })
}

/** Export path: must be an existing directory the app can write to. */
export function checkExportPath(p: string): PathCheck {
  return checkDir(p, { write: true })
}

function checkDir(p: string, opts: { write: boolean }): PathCheck {
  if (!p || !p.trim()) return { ok: false, error: 'パスが指定されていません' }
  let st
  try {
    st = statSync(p)
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code
    if (code === 'ENOENT' || code === 'ENOTDIR') return { ok: false, error: `フォルダが存在しません: ${p}` }
    return { ok: false, error: `フォルダを確認できません (${code ?? 'unknown'}): ${p}` }
  }
  if (!st.isDirectory()) return { ok: false, error: `フォルダではありません: ${p}` }
  try {
    accessSync(p, constants.R_OK | constants.X_OK)
  } catch {
    return { ok: false, error: `読み取り権限がありません: ${p}` }
  }
  if (opts.write) {
    try {
      accessSync(p, constants.W_OK)
    } catch {
      return { ok: false, error: `書き込み権限がありません: ${p}` }
    }
  }
  return { ok: true }
}

/**
 * Desktop mode only (PHRAPEDIA_DATASET_PATH set): throw a 409 with the reason when the dataset path is unusable,
 * so the UI shows an error instead of the server silently creating directories at a wrong place.
 * CLI mode (env unset) is never checked, keeping the legacy behaviour.
 */
export function assertDatasetUsable(): void {
  if (!process.env.PHRAPEDIA_DATASET_PATH) return
  const c = checkDatasetPath(datasetRoot())
  if (!c.ok) throw createError({ statusCode: 409, statusMessage: 'Dataset path unusable', message: `データセットパスが使えません: ${c.error}` })
}

/** Same for the export path; used before any write. */
export function assertExportUsable(): void {
  if (!process.env.PHRAPEDIA_DATASET_PATH && !process.env.PHRAPEDIA_EXPORT_PATH) return
  const c = checkExportPath(exportRoot())
  if (!c.ok) throw createError({ statusCode: 409, statusMessage: 'Export path unusable', message: `書き出し先パスが使えません: ${c.error}` })
}
