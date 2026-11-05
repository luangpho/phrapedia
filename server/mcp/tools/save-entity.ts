import { z } from 'zod'

export default defineMcpTool({
  name: 'save_entity',
  title: 'Create or update a record',
  description: [
    'Write a record as a markdown file under data/<kind>/<slug>.md. Pass the FULL markdown including the YAML frontmatter',
    '(slug, kind, name, name_th, name_en, aliases, temple, province, born/died or founded, status, sources, tags).',
    'Omit slug to create a new record (the slug is derived from name_en / name). Existing files are overwritten, so read first with get_entity when updating.',
  ].join(' '),
  inputSchema: {
    kind: z.enum(['monks', 'temples', 'amulets']),
    slug: z.string().optional().describe('Existing slug to update, or omit to create'),
    markdown: z.string().describe('Full markdown with frontmatter'),
    message: z.string().optional().describe('Commit message hint (used when auto-commit is enabled)'),
  },
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: true },
  handler: async ({ kind, slug, markdown, message }) => {
    const { data } = parseFrontmatter<Record<string, unknown>>(markdown)
    const finalSlug = slug ? assertSlug(slug) : await deriveSlug(kind, data as any)
    const rec = await writeEntity(kind, finalSlug, markdown)
    const commit = await autoCommitIfEnabled(message || `data(${kind}): ${rec.name}`)
    return { ok: true, kind, slug: finalSlug, path: rec.path, name: rec.name, status: rec.status, committed: commit?.ok ?? false }
  },
})
