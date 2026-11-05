import { z } from 'zod'

export default defineMcpTool({
  name: 'web_research',
  title: 'Web research via Google CLI (Antigravity / Gemini)',
  description: [
    'Research a question on the web using the configured Google CLI (Antigravity CLI `agy` by default, or the legacy Gemini CLI) with Google Search. Use ONLY for information that is missing from or unclear in the primary sources.',
    'The answer is markdown with source URLs; everything it returns must be treated as UNVERIFIED and labelled as such in records.',
    'Results are cached per query.',
  ].join(' '),
  inputSchema: {
    query: z.string().describe('What to find out (be specific: name in Thai, temple, province, period)'),
    context: z.string().optional().describe('Known facts from primary sources, so the web search complements rather than contradicts them'),
    noCache: z.boolean().optional().describe('Skip the research cache'),
  },
  annotations: { readOnlyHint: true, openWorldHint: true },
  handler: async ({ query, context, noCache }) => {
    const res = await runGemini({ prompt: buildResearchPrompt(query, context), noCache })
    return `# Web research (UNVERIFIED — ${res.engine ?? 'Gemini'}${res.model ? ` ${res.model}` : ''}${res.cached ? ', cached' : ''})\n\n${res.text}`
  },
})
