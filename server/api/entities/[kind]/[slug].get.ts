export default defineEventHandler(async (event) => {
  const kind = assertKind(getRouterParam(event, 'kind'))
  const slug = assertSlug(getRouterParam(event, 'slug'))
  const rec = await getEntity(kind, slug)
  if (!rec) throw createError({ statusCode: 404, statusMessage: 'Not found', message: `${kind}/${slug} not found` })
  return rec
})
