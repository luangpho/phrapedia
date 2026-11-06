import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { dataDir, exportDataDir, sourcesDir } from '../server/utils/settings'

// Layout A: dataset path and export path are both roots holding data/<kind>/ and sources/ (same as PHRAPEDIA_ROOT).
let dir: string
const saved = { ...process.env }
const g = globalThis as Record<string, unknown>
const savedUseDb = g.useDb

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'phrapedia-layout-'))
  mkdirSync(join(dir, 'ds'))
  mkdirSync(join(dir, 'ex'))
  // getSettings() reads the DB; no stored overrides -> defaults (dataDir 'data', sourcesDir 'sources')
  g.useDb = async () => ({ query: async () => ({ rows: [] }) })
})
afterEach(() => {
  process.env = { ...saved }
  g.useDb = savedUseDb
  rmSync(dir, { recursive: true, force: true })
})

describe('A layout (root with data/<kind>/ and sources/)', () => {
  it('reads data/ and sources/ under the dataset path', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    expect(await dataDir()).toBe(join(dir, 'ds', 'data'))
    expect(await sourcesDir()).toBe(join(dir, 'ds', 'sources'))
  })
  it('writes to data/ under the export path when it differs', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    process.env.PHRAPEDIA_EXPORT_PATH = join(dir, 'ex')
    expect(await exportDataDir()).toBe(join(dir, 'ex', 'data'))
  })
  it('export defaults to the dataset data/ dir', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    expect(await exportDataDir()).toBe(join(dir, 'ds', 'data'))
  })
  it('rejects an unusable export path with 409', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    process.env.PHRAPEDIA_EXPORT_PATH = join(dir, 'missing')
    await expect(exportDataDir()).rejects.toMatchObject({ statusCode: 409 })
  })
})
