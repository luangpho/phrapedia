/**
 * Server utils rely on Nitro auto-imported globals. Define the ones the unit tests need on globalThis:
 * a minimal createError / useRuntimeConfig, and the real pure helpers imported by relative path.
 * `dataDir` is stubbed per test via `setTestDataDir()`.
 */
import { tmpdir } from 'node:os'

export class TestH3Error extends Error {
  statusCode: number
  statusMessage?: string
  constructor(opts: { statusCode?: number; statusMessage?: string; message?: string }) {
    super(opts.message ?? opts.statusMessage ?? 'Error')
    this.statusCode = opts.statusCode ?? 500
    this.statusMessage = opts.statusMessage
  }
}

const g = globalThis as Record<string, unknown>

g.createError = (opts: { statusCode?: number; statusMessage?: string; message?: string } | string) =>
  new TestH3Error(typeof opts === 'string' ? { message: opts } : opts)

g.useRuntimeConfig = () => ({
  anthropicApiKey: '',
  anthropicModel: 'test-model',
  dataDir: 'data',
  sourcesDir: 'sources',
  stateDir: '.phrapedia',
  geminiCommand: 'gemini',
  public: { appName: 'Phrapedia' },
})

const frontmatter = await import('../server/utils/frontmatter')
g.parseFrontmatter = frontmatter.parseFrontmatter
g.stringifyFrontmatter = frontmatter.stringifyFrontmatter

const paths = await import('../server/utils/paths')
g.projectRoot = paths.projectRoot
g.resolveFromRoot = paths.resolveFromRoot
g.isInside = paths.isInside
g.safeJoin = paths.safeJoin
g.toPosix = paths.toPosix
g.stateDir = paths.stateDir

let testDataDir = tmpdir()
export function setTestDataDir(dir: string): void {
  testDataDir = dir
}
g.dataDir = async () => testDataDir
g.sourcesDir = async () => testDataDir
g.getSettings = async () => { throw new Error('getSettings is not available in unit tests') }
g.useDb = async () => { throw new Error('useDb is not available in unit tests') }
