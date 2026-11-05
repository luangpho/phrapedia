/** DELETE /api/chat/sessions/:id */
export default defineEventHandler(async (event) => {
  const id = getRouterParam(event, 'id')
  if (!id) throw createError({ statusCode: 400, message: 'id is required' })
  const deleted = await deleteChatSession(id)
  if (!deleted) throw createError({ statusCode: 404, statusMessage: 'Not found', message: `Chat session not found: ${id}` })
  return { ok: true }
})
