/** GET /api/chat/sessions/:id — { session, messages } */
export default defineEventHandler(async (event) => {
  const id = getRouterParam(event, 'id')
  if (!id) throw createError({ statusCode: 400, message: 'id is required' })
  const session = await getChatSession(id)
  if (!session) throw createError({ statusCode: 404, statusMessage: 'Not found', message: `Chat session not found: ${id}` })
  const messages = await listChatMessages(id)
  return { session, messages }
})
