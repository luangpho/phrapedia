import { z } from 'zod'

export default defineMcpTool({
  name: 'entity_template',
  title: 'Record template',
  description: 'Return the starter markdown (frontmatter + section headings) for a new record of the given kind.',
  inputSchema: {
    kind: z.enum(['monks', 'temples', 'amulets']),
    name: z.string().optional().describe('Display name to pre-fill'),
  },
  annotations: { readOnlyHint: true },
  handler: async ({ kind, name }) => entityTemplate(kind, name),
})
