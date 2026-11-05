import { z } from 'zod'

export default defineMcpTool({
  name: 'get_entity',
  title: 'Read a record',
  description: 'Return the full markdown (YAML frontmatter + body) of one record.',
  inputSchema: {
    kind: z.enum(['monks', 'temples', 'amulets']),
    slug: z.string().describe('Record slug (file name without .md)'),
  },
  annotations: { readOnlyHint: true },
  handler: async ({ kind, slug }) => {
    const rec = await getEntity(kind, assertSlug(slug))
    if (!rec) throw createError({ statusCode: 404, message: `${kind}/${slug} not found` })
    return { kind, slug, path: rec.path, name: rec.name, status: rec.status, markdown: rec.raw }
  },
})
