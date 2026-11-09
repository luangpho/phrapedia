/** DELETE /api/chat/sessions/:id/messages — body { ids: string[] }: drop messages (used when retrying a failed turn). */
export default defineEventHandler(async (event) => {
  const id = getRouterParam(event, 'id')
  if (!id) throw createError({ statusCode: 400, message: 'id is required' })
  const body = await readBody<{ ids?: unknown }>(event)
  const ids = Array.isArray(body?.ids) ? body!.ids.filter((x): x is string => typeof x === 'string').slice(0, 100) : []
  if (!ids.length) return { ok: true, deleted: 0 }
  const session = await getChatSession(id)
  if (!session) throw createError({ statusCode: 404, statusMessage: 'Not found', message: `Chat session not found: ${id}` })
  const deleted = await deleteChatMessages(id, ids)
  // The CLI session still remembers the removed turns: start a fresh one (history is re-sent as a summary).
  if (deleted > 0) await setClaudeSessionId(id, null)
  return { ok: true, deleted }
})
