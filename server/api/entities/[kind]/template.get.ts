export default defineEventHandler(async (event) => {
  const kind = assertKind(getRouterParam(event, 'kind'))
  const q = getQuery(event)
  return { raw: entityTemplate(kind, typeof q.name === 'string' ? q.name : '') }
})
