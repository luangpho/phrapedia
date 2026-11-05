import { ResourceTemplate } from '@modelcontextprotocol/sdk/server/mcp.js'
import type { Variables } from '@modelcontextprotocol/sdk/shared/uriTemplate.js'

export default defineMcpResource({
  name: 'entity',
  title: 'Phrapedia record',
  description: 'Markdown record of a monk, temple or amulet: phrapedia://{kind}/{slug}',
  uri: new ResourceTemplate('phrapedia://{kind}/{slug}', {
    list: async () => {
      const resources = []
      for (const kind of ['monks', 'temples', 'amulets'] as const) {
        for (const e of await listEntities(kind)) {
          resources.push({ uri: `phrapedia://${kind}/${encodeURIComponent(e.slug)}`, name: e.name, mimeType: 'text/markdown' })
        }
      }
      return { resources }
    },
  }),
  handler: async (uri: URL, variables: Variables) => {
    const kind = assertKind(String(variables.kind))
    const slug = assertSlug(String(variables.slug))
    const rec = await getEntity(kind, slug)
    if (!rec) throw createError({ statusCode: 404, message: `${kind}/${slug} not found` })
    return { contents: [{ uri: uri.href, mimeType: 'text/markdown', text: rec.raw }] }
  },
})
