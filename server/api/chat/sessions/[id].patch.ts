/** PATCH /api/chat/sessions/:id — { slug?, title? }: re-bind a draft conversation to the saved record, or rename it. */
export default defineEventHandler(async (event) => {
  const id = getRouterParam(event, 'id')
  if (!id) throw createError({ statusCode: 400, message: 'id is required' })
  const body = await readBody<{ slug?: string | null; title?: string }>(event)
  const session = await getChatSession(id)
  if (!session) throw createError({ statusCode: 404, statusMessage: 'Not found', message: `Chat session not found: ${id}` })
  const patch: { slug?: string | null; title?: string } = {}
  if (body && 'slug' in body) patch.slug = body.slug ? assertSlug(body.slug) : null
  if (body && typeof body.title === 'string') patch.title = body.title
  await touchChatSession(id, patch)
  return await getChatSession(id)
})
