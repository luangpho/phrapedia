export default defineEventHandler(async () => {
  const db = await useDb()
  const r = await db.query<{ now: string }>('SELECT now()::text AS now')
  const desktop = Boolean(process.env.PHRAPEDIA_DATASET_PATH)
  return {
    ok: true,
    db: r.rows[0]?.now,
    root: projectRoot(),
    // Desktop (sidecar) mode: the configured paths and whether they are usable (the app starts either way).
    paths: {
      desktop,
      dataset: { path: datasetRoot(), ...checkDatasetPath(datasetRoot()) },
      export: { path: exportRoot(), ...checkExportPath(exportRoot()) },
    },
  }
})
