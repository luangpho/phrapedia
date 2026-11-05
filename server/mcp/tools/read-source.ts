import { z } from 'zod'

export default defineMcpTool({
  name: 'read_source',
  title: 'Read a primary source',
  description: 'Read a file from the primary-sources directory. Markdown/text is returned as text; images are returned as image content so they can be inspected.',
  inputSchema: {
    path: z.string().describe('Path relative to the sources directory, as returned by list_sources / search_sources'),
    maxChars: z.number().int().min(100).max(200_000).optional().describe('Truncate text after this many characters (default 40000)'),
  },
  annotations: { readOnlyHint: true },
  handler: async ({ path, maxChars }) => {
    const src = await readSource(path, { maxChars: maxChars ?? 40_000 })
    if (src.kind === 'image' && src.base64 && src.mimeType) {
      return { content: [{ type: 'text', text: `Image source: ${src.path}` }, { type: 'image', data: src.base64, mimeType: src.mimeType }] }
    }
    if (src.text !== undefined) return `# Source: ${src.path}\n\n${src.text}`
    return `Source ${src.path} is a ${src.kind} file (${src.size} bytes); its content cannot be read as text.`
  },
})
