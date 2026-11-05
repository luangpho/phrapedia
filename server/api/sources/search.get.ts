export default defineEventHandler(async (event) => {
  const q = getQuery(event)
  const query = typeof q.q === 'string' ? q.q : ''
  const limit = Math.min(50, Math.max(1, Number(q.limit) || 20))
  if (!query.trim()) return []
  return searchSources(query, limit)
})
