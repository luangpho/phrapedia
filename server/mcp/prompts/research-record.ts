import { z } from 'zod'

export default defineMcpPrompt({
  name: 'research_record',
  title: 'Research and write a record',
  description: 'Guided workflow: gather primary sources, fill gaps with web research, and save a monk or temple record.',
  inputSchema: {
    kind: z.enum(['monks', 'temples']),
    name: z.string().describe('Name of the monk or temple (any language)'),
  },
  handler: async ({ kind, name }) => {
    const label = kind === 'monks' ? '高僧' : '寺院'
    return [
      `「${name}」という${label}のレコードを作成してください。手順:`,
      `1. search_sources / read_source で一次資料（確定情報）を集める。画像資料も read_source で確認する。`,
      `2. 一次資料で不明な点だけ web_research で調べ、結果は「未確認」として扱う。`,
      `3. entity_template(kind="${kind}") の構成に沿って Markdown を作り、出典セクションに一次資料パスと Web URL を分けて記載する。`,
      `4. save_entity で保存する（既存なら list_entities で slug を確認し更新）。`,
    ].join('\n')
  },
})
