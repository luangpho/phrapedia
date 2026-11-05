export default defineEventHandler(async () => {
  const db = await useDb()
  const r = await db.query<{ now: string }>('SELECT now()::text AS now')
  return { ok: true, db: r.rows[0]?.now, root: projectRoot() }
})
