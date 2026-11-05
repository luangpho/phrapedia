#!/usr/bin/env node
/**
 * Smoke test for the production build.
 *   npx nuxt build && npm run smoke
 *
 * Boots .output/server/index.mjs on a random free port (PHRAPEDIA_ROOT = repo root, PGlite state in a temp dir
 * so a running dev server is not disturbed), runs a few REST + MCP checks, prints PASS/FAIL per check and exits
 * non-zero on any failure.
 */
import { spawn } from 'node:child_process'
import { createServer } from 'node:net'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const entry = join(root, '.output', 'server', 'index.mjs')
if (!existsSync(entry)) {
  console.error(`FAIL  build output not found: ${entry} (run "npx nuxt build" first)`)
  process.exit(1)
}

function freePort() {
  return new Promise((res, rej) => {
    const srv = createServer()
    srv.unref()
    srv.on('error', rej)
    srv.listen(0, '127.0.0.1', () => {
      const { port } = srv.address()
      srv.close(() => res(port))
    })
  })
}

const port = await freePort()
const base = `http://127.0.0.1:${port}`
const stateDir = mkdtempSync(join(tmpdir(), 'phrapedia-smoke-'))

let serverLog = ''
const server = spawn(process.execPath, [entry], {
  cwd: root,
  env: {
    ...process.env,
    PORT: String(port),
    HOST: '127.0.0.1',
    NITRO_PORT: String(port),
    NITRO_HOST: '127.0.0.1',
    PHRAPEDIA_ROOT: root,
    NUXT_STATE_DIR: stateDir,
    NUXT_DATA_DIR: 'data',
    NUXT_SOURCES_DIR: 'sources',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
})
server.stdout.on('data', d => { serverLog += d })
server.stderr.on('data', d => { serverLog += d })

let exited = false
server.on('exit', () => { exited = true })

function cleanup() {
  if (!exited) server.kill('SIGTERM')
  try { rmSync(stateDir, { recursive: true, force: true }) } catch { /* ignore */ }
}
process.on('SIGINT', () => { cleanup(); process.exit(130) })

const results = []
async function check(name, fn) {
  const t0 = Date.now()
  try {
    const detail = await fn()
    results.push({ name, ok: true })
    console.log(`PASS  ${name}${detail ? ` — ${detail}` : ''} (${Date.now() - t0} ms)`)
  } catch (err) {
    results.push({ name, ok: false })
    console.log(`FAIL  ${name} — ${err instanceof Error ? err.message : String(err)}`)
  }
}
function assert(cond, msg) {
  if (!cond) throw new Error(msg)
}

async function getJson(path) {
  const res = await fetch(base + path, { headers: { accept: 'application/json' } })
  const text = await res.text()
  assert(res.ok, `${path} -> HTTP ${res.status}: ${text.slice(0, 300)}`)
  return JSON.parse(text)
}

let rpcId = 0
async function mcp(method, params = {}) {
  const res = await fetch(`${base}/mcp`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream' },
    body: JSON.stringify({ jsonrpc: '2.0', id: ++rpcId, method, params }),
  })
  const text = await res.text()
  assert(res.ok, `/mcp ${method} -> HTTP ${res.status}: ${text.slice(0, 300)}`)
  let msg
  if ((res.headers.get('content-type') || '').includes('text/event-stream')) {
    const data = text.split('\n').filter(l => l.startsWith('data:')).map(l => l.slice(5).trim()).filter(Boolean)
    msg = JSON.parse(data[data.length - 1])
  } else {
    msg = JSON.parse(text)
  }
  assert(!msg.error, `/mcp ${method} error: ${JSON.stringify(msg.error)}`)
  return msg.result
}

try {
  await check('server boots and /api/health responds', async () => {
    const deadline = Date.now() + 60_000
    let last = ''
    while (Date.now() < deadline) {
      if (exited) throw new Error(`server exited early:\n${serverLog.slice(-2000)}`)
      try {
        const res = await fetch(`${base}/api/health`)
        if (res.ok) {
          const body = await res.json()
          assert(body.ok === true, `unexpected health body ${JSON.stringify(body)}`)
          return `port ${port}`
        }
        last = `HTTP ${res.status}`
      } catch (err) {
        last = err.message
      }
      await new Promise(r => setTimeout(r, 300))
    }
    throw new Error(`timed out waiting for /api/health (${last})`)
  })

  if (results[0]?.ok) {
    await check('GET /api/entities/monks lists the sample records', async () => {
      const list = await getJson('/api/entities/monks')
      assert(Array.isArray(list), 'not an array')
      const to = list.find(e => e.slug === 'somdet-to')
      const thuat = list.find(e => e.slug === 'luang-pu-thuat')
      assert(to && to.name === 'ソムデット・トー', `somdet-to missing or wrong name: ${JSON.stringify(to)}`)
      assert(thuat && thuat.name === 'ルアンプー・トゥアット', `luang-pu-thuat missing or wrong name: ${JSON.stringify(thuat)}`)
      return `${list.length} records`
    })

    await check('GET /api/entities/temples lists the sample records', async () => {
      const list = await getJson('/api/entities/temples')
      for (const [slug, name] of [['wat-rakhang', 'ワット・ラカン'], ['wat-chang-hai', 'ワット・チャーンハイ']]) {
        const e = list.find(x => x.slug === slug)
        assert(e && e.name === name, `${slug} missing or wrong name: ${JSON.stringify(e)}`)
      }
      return `${list.length} records`
    })

    await check('GET /api/entities/monks/somdet-to returns markdown with the Thai name', async () => {
      const rec = await getJson('/api/entities/monks/somdet-to')
      assert(typeof rec.raw === 'string' && rec.raw.includes('สมเด็จพระพุฒาจารย์'), 'Thai name not in raw markdown')
      assert(rec.frontmatter?.temple === 'wat-rakhang', `temple = ${rec.frontmatter?.temple}`)
      return rec.path
    })

    await check('GET /api/sources/search?q=ラカン returns a hit', async () => {
      const hits = await getJson(`/api/sources/search?q=${encodeURIComponent('ラカン')}`)
      assert(Array.isArray(hits) && hits.length > 0, 'no hits')
      return hits.map(h => h.path).join(', ')
    })

    await check('POST /mcp initialize', async () => {
      const r = await mcp('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'phrapedia-smoke', version: '0' } })
      return `${r.serverInfo?.name} ${r.serverInfo?.version}`
    })

    await check('POST /mcp tools/list contains save_entity and web_research', async () => {
      const r = await mcp('tools/list')
      const names = (r.tools || []).map(t => t.name)
      for (const n of ['save_entity', 'web_research', 'search_sources', 'get_entity']) assert(names.includes(n), `missing ${n} in [${names.join(', ')}]`)
      return `${names.length} tools`
    })

    await check('POST /mcp tools/call search_sources works', async () => {
      const r = await mcp('tools/call', { name: 'search_sources', arguments: { query: 'ソムデット', limit: 5 } })
      assert(!r.isError, `tool error: ${JSON.stringify(r.content)}`)
      const text = (r.content || []).filter(c => c.type === 'text').map(c => c.text).join('\n')
      const payload = r.structuredContent ?? JSON.parse(text)
      assert(payload.count > 0, `no hits: ${text.slice(0, 300)}`)
      return `${payload.count} hits`
    })
  }
} finally {
  cleanup()
}

const failed = results.filter(r => !r.ok).length
console.log(`\n${failed ? 'FAIL' : 'PASS'}  ${results.length - failed}/${results.length} checks passed`)
if (failed && serverLog.trim()) console.log(`\n--- server log (tail) ---\n${serverLog.slice(-3000)}`)
process.exit(failed ? 1 : 0)
