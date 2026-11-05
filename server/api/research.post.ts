/** Direct web research (Antigravity / Gemini CLI) without the chat agent. Body: { query, context?, noCache? } */
export default defineEventHandler(async (event) => {
  const body = await readBody<{ query?: string; context?: string; images?: string[]; ocrText?: string; noCache?: boolean }>(event)
  if (!body?.query?.trim()) throw createError({ statusCode: 400, message: 'query is required' })
  const images = Array.isArray(body.images) ? body.images.filter((p): p is string => typeof p === 'string').slice(0, 8) : []
  const res = await runGemini({ prompt: buildResearchPrompt(body.query, { context: body.context, images, ocrText: body.ocrText }), images, noCache: body.noCache })
  return res
})
