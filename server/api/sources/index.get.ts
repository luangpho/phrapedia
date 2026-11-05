export default defineEventHandler(async (event) => {
  const q = getQuery(event)
  const subdir = typeof q.dir === 'string' ? q.dir : ''
  return listSources(subdir)
})
