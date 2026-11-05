import type { SaveEntityInput } from '../../../../shared/types/entity'

/** Update an existing record. Body: { raw, message? } */
export default defineEventHandler(async (event) => {
  const kind = assertKind(getRouterParam(event, 'kind'))
  const slug = assertSlug(getRouterParam(event, 'slug'))
  const body = await readBody<SaveEntityInput>(event)
  if (!body?.raw || typeof body.raw !== 'string') throw createError({ statusCode: 400, message: 'raw (markdown) is required' })
  const rec = await writeEntity(kind, slug, body.raw, { mustExist: true })
  const commit = await autoCommitIfEnabled(body.message || `data(${kind}): update ${rec.name}`)
  return { record: rec, commit }
})
