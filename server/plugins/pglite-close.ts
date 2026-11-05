import type { PGlite } from '@electric-sql/pglite'

/**
 * PGlite has no data-directory lock: two live instances on one dataDir lose writes.
 * Nitro's dev worker is replaced on every server/** change (old and new worker overlap briefly),
 * so close the database on the `close` hook. The same hook runs on SIGTERM/SIGINT in production.
 */
export default defineNitroPlugin((nitroApp) => {
  nitroApp.hooks.hook('close', async () => {
    const g = globalThis as typeof globalThis & { __phrapediaDb?: Promise<PGlite> }
    const pending = g.__phrapediaDb
    g.__phrapediaDb = undefined
    if (pending) {
      try {
        const db = await pending
        await db.close()
      } catch {
        // already closed or failed to open
      }
    }
  })
})
