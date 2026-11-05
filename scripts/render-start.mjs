#!/usr/bin/env node
/**
 * Production start script for Render (and any host with a persistent disk).
 *
 * 1. If NUXT_DATA_DIR / NUXT_SOURCES_DIR point outside the repository and are still empty,
 *    seed them from the repository's data/ and sources/ folders (sample records, README).
 * 2. Start the Nitro server (.output/server/index.mjs). PORT/HOST come from the environment.
 */
import { cp, mkdir, readdir, stat, readFile, writeFile } from 'node:fs/promises'
import { homedir } from 'node:os'
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, resolve, isAbsolute } from 'node:path'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const resolveDir = (p, fallback) => (p ? (isAbsolute(p) ? p : resolve(root, p)) : resolve(root, fallback))

async function isEmptyOrMissing(dir) {
  try {
    const st = await stat(dir)
    if (!st.isDirectory()) return false
    return (await readdir(dir)).length === 0
  } catch {
    return true
  }
}

async function seed(name, target) {
  const source = resolve(root, name)
  if (resolve(target) === source) return
  if (!(await isEmptyOrMissing(target))) return
  await mkdir(target, { recursive: true })
  try {
    await cp(source, target, { recursive: true })
    console.log(`[phrapedia] seeded ${target} from ${source}`)
  } catch (err) {
    console.warn(`[phrapedia] could not seed ${target}: ${err.message}`)
  }
}

const dataDir = resolveDir(process.env.NUXT_DATA_DIR, 'data')
const sourcesDir = resolveDir(process.env.NUXT_SOURCES_DIR, 'sources')
const stateDir = resolveDir(process.env.NUXT_STATE_DIR, '.phrapedia')

await seed('data', dataDir)
await seed('sources', sourcesDir)
await mkdir(stateDir, { recursive: true })
for (const extra of [process.env.GEMINI_CLI_HOME]) {
  if (extra) await mkdir(extra, { recursive: true })
}

// Antigravity CLI: headless auth with an API key needs modelProvider: "gemini" in its settings.json
// (https://github.com/google-antigravity/antigravity-cli CHANGELOG 1.1.13). Merge it in when a key is present.
if (process.env.GEMINI_API_KEY && (process.env.NUXT_GEMINI_ENGINE || 'antigravity') === 'antigravity') {
  const dir = resolve(process.env.GEMINI_CLI_HOME || resolve(homedir(), '.gemini'), 'antigravity-cli')
  const file = resolve(dir, 'settings.json')
  try {
    await mkdir(dir, { recursive: true })
    let current = {}
    try { current = JSON.parse(await readFile(file, 'utf8')) } catch { /* new file */ }
    if (current.modelProvider !== 'gemini') {
      await writeFile(file, JSON.stringify({ ...current, modelProvider: 'gemini' }, null, 2) + '\n')
      console.log(`[phrapedia] set modelProvider=gemini in ${file}`)
    }
  } catch (err) {
    console.warn(`[phrapedia] could not prepare agy settings: ${err.message}`)
  }
}

const child = spawn(process.execPath, [resolve(root, '.output/server/index.mjs')], {
  cwd: root,
  stdio: 'inherit',
  env: { ...process.env, PHRAPEDIA_ROOT: process.env.PHRAPEDIA_ROOT || root },
})
for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => child.kill(sig))
child.on('exit', code => process.exit(code ?? 0))
