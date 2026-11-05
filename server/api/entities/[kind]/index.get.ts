export default defineEventHandler(async (event) => {
  const kind = assertKind(getRouterParam(event, 'kind'))
  return listEntities(kind)
})
