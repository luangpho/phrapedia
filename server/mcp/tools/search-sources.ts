import { z } from 'zod'

export default defineMcpTool({
  name: 'search_sources',
  title: 'Search primary sources',
  description: 'Full-text (substring, case-insensitive) search over markdown/text files in the primary-sources directory. Thai, Japanese and English terms all work. Returns paths with a snippet; follow up with read_source.',
  inputSchema: {
    query: z.string().describe('Search terms separated by spaces (name in Thai / Japanese / English, temple, province, ...)'),
    limit: z.number().int().min(1).max(50).optional().describe('Max hits (default 20)'),
  },
  annotations: { readOnlyHint: true },
  handler: async ({ query, limit }) => {
    const hits = await searchSources(query, limit ?? 20)
    return { query, count: hits.length, hits }
  },
})
