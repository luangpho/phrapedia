import { z } from 'zod'

export default defineMcpTool({
  name: 'sync_data',
  title: 'Commit and sync records with GitHub',
  description: 'Commit all changes under the data directory, pull --rebase from the configured remote branch, then push. Only the data directory is staged.',
  inputSchema: {
    message: z.string().optional().describe('Commit message'),
    mode: z.enum(['commit', 'pull', 'push', 'sync']).optional().describe('Default: sync (commit + pull + push)'),
  },
  annotations: { readOnlyHint: false, destructiveHint: false, openWorldHint: true },
  handler: async ({ message, mode }) => {
    switch (mode) {
      case 'commit': return toToolJson(await commitData(message))
      case 'pull': return toToolJson(await pullData())
      case 'push': return toToolJson(await pushData())
      default: return toToolJson(await syncData(message))
    }
  },
})
