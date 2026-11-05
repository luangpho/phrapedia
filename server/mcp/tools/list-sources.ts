import { z } from 'zod'

export default defineMcpTool({
  name: 'list_sources',
  title: 'List primary sources',
  description: 'List files in the primary-sources directory (markdown notes, images, PDFs). Primary sources are authoritative; prefer them over web research.',
  inputSchema: {
    subdir: z.string().optional().describe('Sub-directory to list (relative to the sources directory)'),
  },
  annotations: { readOnlyHint: true },
  handler: async ({ subdir }) => {
    const files = await listSources(subdir ?? '')
    return { count: files.length, files: files.map(f => ({ path: f.path, kind: f.kind, size: f.size, mtime: f.mtime })) }
  },
})
