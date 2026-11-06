#!/usr/bin/env node
// cask の version と sha256 (arm / intel) を書き換える。
// 使い方:
//   node update-cask.mjs --version 0.1.0 --arm-dmg A.dmg --intel-dmg B.dmg [--cask path]
//   node update-cask.mjs --version 0.1.0 --arm-sha <hex> --intel-sha <hex> [--cask path]
import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

function fail(msg) {
  console.error(`update-cask: ${msg}`)
  process.exit(1)
}

const args = {}
const argv = process.argv.slice(2)
for (let i = 0; i < argv.length; i += 2) {
  if (!argv[i].startsWith('--') || argv[i + 1] === undefined) fail(`invalid argument: ${argv[i]}`)
  args[argv[i].slice(2)] = argv[i + 1]
}

const sha = (dmg, direct, label) => {
  const value = direct ?? (dmg ? createHash('sha256').update(readFileSync(dmg)).digest('hex') : undefined)
  if (!value) fail(`--${label}-dmg or --${label}-sha is required`)
  if (!/^[0-9a-f]{64}$/.test(value)) fail(`invalid sha256 for ${label}: ${value}`)
  return value
}

const version = (args.version ?? '').replace(/^v/, '')
if (!/^\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?$/.test(version)) fail(`invalid --version: ${args.version}`)
const arm = sha(args['arm-dmg'], args['arm-sha'], 'arm')
const intel = sha(args['intel-dmg'], args['intel-sha'], 'intel')

const here = dirname(fileURLToPath(import.meta.url))
const caskPath = args.cask ?? join(here, 'Casks', 'phrapedia.rb')
const src = readFileSync(caskPath, 'utf8')

const out = src
  .replace(/^(\s*version\s+")[^"]*(")/m, `$1${version}$2`)
  .replace(/(sha256\s+arm:\s+")[0-9a-f]*(")/, `$1${arm}$2`)
  .replace(/(intel:\s+")[0-9a-f]*(")/, `$1${intel}$2`)

for (const needle of [`version "${version}"`, `arm:   "${arm}"`, `intel: "${intel}"`]) {
  if (!out.includes(needle)) fail(`cask format unexpected; could not set: ${needle}`)
}
writeFileSync(caskPath, out)
console.log(`updated ${caskPath}: version=${version} arm=${arm.slice(0, 12)} intel=${intel.slice(0, 12)}`)
