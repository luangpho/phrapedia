import { z } from 'zod'

export const UPDATE_DRAFT_ACK = 'draft updated (not saved yet)'

/**
 * In-app chat only: the chat runner (server/utils/agent) watches the `claude -p` stream for calls to this tool and
 * forwards the markdown to the editor as a `draft` event. The tool itself changes nothing on the server, so
 * external MCP clients calling it are harmless.
 */
export default defineMcpTool({
  name: 'update_draft',
  title: 'Update the editor draft (in-app chat only)',
  description: [
    'For the Phrapedia in-app chat ONLY: replace the working draft shown in the user\'s editor with new content.',
    'Pass the FULL updated markdown (YAML frontmatter + body), never a diff or a fragment.',
    'This does NOT save any file; the user saves it. Use it whenever you add or change record content.',
    'Calling it from an external MCP client has no effect (it only returns an acknowledgement).',
  ].join(' '),
  inputSchema: {
    markdown: z.string().min(1).describe('Full markdown of the record including the YAML frontmatter'),
    summary: z.string().optional().describe('One-line summary (Japanese) of what changed'),
  },
  annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true },
  handler: async () => UPDATE_DRAFT_ACK,
})
