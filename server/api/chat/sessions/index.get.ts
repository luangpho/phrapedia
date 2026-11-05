/** GET /api/chat/sessions?kind=&slug= — sessions of a record (or drafts when slug is omitted), newest first. */
export default defineEventHandler(async (event) => {
  const q = getQuery(event)
  const kind = assertKind(typeof q.kind === 'string' ? q.kind : undefined)
  const slug = typeof q.slug === 'string' && q.slug ? assertSlug(q.slug) : null
  return listChatSessions(kind, slug)
})
