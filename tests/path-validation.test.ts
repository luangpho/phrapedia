import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { assertDatasetUsable, assertExportUsable, checkDatasetPath, checkExportPath } from '../server/utils/path-validation'
import { datasetRoot, exportRoot, projectRoot } from '../server/utils/paths'

let dir: string
const saved = { ...process.env }

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'phrapedia-pv-'))
})
afterEach(() => {
  process.env = { ...saved }
  try { chmodSync(dir, 0o700) } catch { /* removed */ }
  rmSync(dir, { recursive: true, force: true })
})

describe('checkDatasetPath / checkExportPath', () => {
  it('accepts an existing directory', () => {
    expect(checkDatasetPath(dir)).toEqual({ ok: true })
    expect(checkExportPath(dir)).toEqual({ ok: true })
  })
  it('rejects empty and missing paths', () => {
    expect(checkDatasetPath('').ok).toBe(false)
    const missing = checkDatasetPath(join(dir, 'nope'))
    expect(missing.ok).toBe(false)
    expect(missing.error).toContain('存在しません')
  })
  it('rejects a file', () => {
    const f = join(dir, 'f.txt')
    writeFileSync(f, 'x')
    expect(checkDatasetPath(f).error).toContain('フォルダではありません')
  })
  it.skipIf(process.getuid?.() === 0)('rejects a read-only export path but allows it as dataset', () => {
    chmodSync(dir, 0o500)
    expect(checkDatasetPath(dir).ok).toBe(true)
    const r = checkExportPath(dir)
    expect(r.ok).toBe(false)
    expect(r.error).toContain('書き込み')
  })
})

describe('roots from environment', () => {
  it('falls back to the project root when unset (CLI mode)', () => {
    delete process.env.PHRAPEDIA_DATASET_PATH
    delete process.env.PHRAPEDIA_EXPORT_PATH
    expect(datasetRoot()).toBe(projectRoot())
    expect(exportRoot()).toBe(projectRoot())
    expect(() => assertDatasetUsable()).not.toThrow()
    expect(() => assertExportUsable()).not.toThrow()
  })
  it('export defaults to the dataset path', () => {
    process.env.PHRAPEDIA_DATASET_PATH = dir
    delete process.env.PHRAPEDIA_EXPORT_PATH
    expect(datasetRoot()).toBe(dir)
    expect(exportRoot()).toBe(dir)
  })
  it('export path can differ', () => {
    process.env.PHRAPEDIA_DATASET_PATH = dir
    process.env.PHRAPEDIA_EXPORT_PATH = join(dir, 'out')
    expect(exportRoot()).toBe(join(dir, 'out'))
  })
  it('asserts throw a 409 for missing paths in desktop mode', () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'missing')
    expect(() => assertDatasetUsable()).toThrowError(/データセットパスが使えません/)
    process.env.PHRAPEDIA_DATASET_PATH = dir
    process.env.PHRAPEDIA_EXPORT_PATH = join(dir, 'missing')
    expect(() => assertExportUsable()).toThrowError(/書き出し先パスが使えません/)
  })
})
