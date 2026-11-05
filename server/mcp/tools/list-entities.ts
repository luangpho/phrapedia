import { z } from 'zod'

export default defineMcpTool({
  name: 'list_entities',
  title: 'List records',
  description: 'List saved records (monks / temples / amulets) with slug, name and status. Use this to find the slug of an existing record before reading or updating it.',
  inputSchema: {
    kind: z.enum(['monks', 'temples', 'amulets']).describe('Record kind'),
  },
  annotations: { readOnlyHint: true },
  handler: async ({ kind }) => {
    const items = await listEntities(kind)
    return { kind, count: items.length, items }
  },
})
