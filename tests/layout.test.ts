import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { dataDir, exportDataDir, sourcesDir } from '../server/utils/settings'

// Layout B: dataset path and export path are the data/-equivalent directories themselves
// (monks/ temples/ amulets/ directly inside; sources/ next to them in the dataset path).
let dir: string
const saved = { ...process.env }

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'phrapedia-layout-'))
  mkdirSync(join(dir, 'ds'))
  mkdirSync(join(dir, 'ex'))
})
afterEach(() => {
  process.env = { ...saved }
  rmSync(dir, { recursive: true, force: true })
})

describe('B layout (data/-equivalent directories)', () => {
  it('reads records from the dataset path itself and sources/ directly inside it', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    expect(await dataDir()).toBe(join(dir, 'ds'))
    expect(await sourcesDir()).toBe(join(dir, 'ds', 'sources'))
  })
  it('writes to the export path itself when it differs', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    process.env.PHRAPEDIA_EXPORT_PATH = join(dir, 'ex')
    expect(await exportDataDir()).toBe(join(dir, 'ex'))
  })
  it('export defaults to the dataset path', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    expect(await exportDataDir()).toBe(join(dir, 'ds'))
  })
  it('rejects an unusable export path with 409', async () => {
    process.env.PHRAPEDIA_DATASET_PATH = join(dir, 'ds')
    process.env.PHRAPEDIA_EXPORT_PATH = join(dir, 'missing')
    await expect(exportDataDir()).rejects.toMatchObject({ statusCode: 409 })
  })
})
