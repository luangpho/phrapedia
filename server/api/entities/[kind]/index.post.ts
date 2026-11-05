import type { SaveEntityInput } from '../../../../shared/types/entity'

/** Create a new record. Body: { raw, message? }. The slug is derived from the frontmatter (slug > name_en > name). */
export default defineEventHandler(async (event) => {
  const kind = assertKind(getRouterParam(event, 'kind'))
  const body = await readBody<SaveEntityInput>(event)
  if (!body?.raw || typeof body.raw !== 'string') throw createError({ statusCode: 400, message: 'raw (markdown) is required' })
  const { data } = parseFrontmatter<Record<string, unknown>>(body.raw)
  const slug = await deriveSlug(kind, data as any)
  const rec = await writeEntity(kind, slug, body.raw, { mustNotExist: true })
  const commit = await autoCommitIfEnabled(body.message || `data(${kind}): add ${rec.name}`)
  return { record: rec, commit }
})
