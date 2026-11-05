export default defineEventHandler(async (event) => {
  const kind = assertKind(getRouterParam(event, 'kind'))
  const slug = assertSlug(getRouterParam(event, 'slug'))
  await deleteEntity(kind, slug)
  const commit = await autoCommitIfEnabled(`data(${kind}): remove ${slug}`)
  return { ok: true, commit }
})
