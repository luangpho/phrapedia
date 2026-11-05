import { z } from 'zod'

export default defineMcpTool({
  name: 'web_research',
  title: 'Web research via Google CLI (Antigravity / Gemini)',
  description: [
    'Research a question on the web using the configured Google CLI (Antigravity CLI `agy` by default, or the legacy Gemini CLI) with Google Search. Use ONLY for information that is missing from or unclear in the primary sources.',
    'When the user\'s request is about an IMAGE from the primary sources ("この画像の…"), you MUST pass the image path(s) in `images` and the text you yourself read from the image in `ocrText`: the CLI then opens the image, reads its text on its own (re-reading enlarged parts when a match is not certain), compares it with your reading, and searches with the extracted strings.',
    'The answer is markdown with source URLs; everything it returns must be treated as UNVERIFIED and labelled as such in records.',
    'Results are cached per query (and per image contents).',
  ].join(' '),
  inputSchema: {
    query: z.string().describe('What to find out (be specific: name in Thai, temple, province, period)'),
    context: z.string().optional().describe('Known facts from primary sources, so the web search complements rather than contradicts them'),
    images: z.array(z.string()).max(8).optional().describe('Primary-source image paths (relative to the sources dir, as returned by list_sources) that the question is about. The images themselves are handed to the CLI.'),
    ocrText: z.string().optional().describe('Text you (the assistant) read from those images, as reference for the CLI. It will re-read the image itself and report differences.'),
    noCache: z.boolean().optional().describe('Skip the research cache'),
  },
  annotations: { readOnlyHint: true, openWorldHint: true },
  handler: async ({ query, context, images, ocrText, noCache }) => {
    const res = await runGemini({ prompt: buildResearchPrompt(query, { context, images, ocrText }), images, noCache })
    return `# Web research (UNVERIFIED — ${res.engine ?? 'Gemini'}${res.model ? ` ${res.model}` : ''}${res.cached ? ', cached' : ''})\n\n${res.text}`
  },
})
