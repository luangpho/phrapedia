/** Direct web research (Gemini CLI) without the chat agent. Body: { query, context?, noCache? } */
export default defineEventHandler(async (event) => {
  const body = await readBody<{ query?: string; context?: string; noCache?: boolean }>(event)
  if (!body?.query?.trim()) throw createError({ statusCode: 400, message: 'query is required' })
  const res = await runGemini({ prompt: buildResearchPrompt(body.query, body.context), noCache: body.noCache })
  return res
})
