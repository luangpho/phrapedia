import type { AppSettings } from '../../../shared/types/settings'
import type { EntityKind } from '../../../shared/types/entity'
import { ENTITY_KIND_LABELS } from '../../../shared/types/entity'

export interface SystemPromptInput {
  kind: EntityKind
  slug: string | null | undefined
  /** Current editor content (full markdown with frontmatter) */
  draft: string
  settings: AppSettings
  /** Names of the tools available to the model */
  toolNames: string[]
}

const FOCUS: Record<EntityKind, string> = {
  monks: [
    '今回の対象は「高僧」のレコードです。高僧本人を中心に調べ、書いてください:',
    '- 生涯（生年・没年・出家・得度・修行・主な事績。仏暦/西暦の別を明記）',
    '- 所属寺院（住職を務めた寺院、修行した寺院。frontmatter の temple には寺院の slug か名称）',
    '- 法脈・師弟関係（師匠・弟子）',
    '- 代表的なプラクルアン（お守り）とその特徴・製作年',
    '寺院そのものの詳しい歴史は寺院レコードに任せ、ここでは高僧との関係に絞ります。',
  ].join('\n'),
  temples: [
    '今回の対象は「寺院」のレコードです。寺院を中心に調べ、書いてください:',
    '- 歴史（創建年・創建者・変遷。frontmatter の founded）',
    '- 所在地（県・郡・住所。frontmatter の province）',
    '- 関連する高僧（歴代住職・ゆかりの高僧。高僧の詳しい生涯は高僧レコードに任せる）',
    '- 寺院ゆかりのプラクルアン',
  ].join('\n'),
  amulets: [
    '今回の対象は「プラクルアン（お守り）」のレコードです。お守りを中心に調べ、書いてください:',
    '- 名称・種類・素材・製作年・製作した高僧と寺院・見分け方',
  ].join('\n'),
}

const FRONTMATTER_FIELDS: Record<EntityKind, string> = {
  monks: 'slug, kind, name（日本語表記）, name_th（タイ語）, name_en（ローマ字）, aliases, temple, province, born, died, status, sources, tags',
  temples: 'slug, kind, name（日本語表記）, name_th（タイ語）, name_en（ローマ字）, aliases, province, founded, status, sources, tags',
  amulets: 'slug, kind, name, name_th, name_en, aliases, temple, status, sources, tags',
}

const HEADINGS: Record<EntityKind, string> = {
  monks: '# <名前> / ## 概要 / ## 生涯 / ## 所属寺院 / ## 代表的なプラクルアン / ## 出典',
  temples: '# <名前> / ## 概要 / ## 歴史 / ## 関連する高僧 / ## 所在地 / ## 出典',
  amulets: '# <名前> / ## 概要 / ## 出典',
}

/** Japanese system prompt for the record-editing assistant. */
export function buildSystemPrompt(input: SystemPromptInput): string {
  const { kind, slug, draft, settings, toolNames } = input
  const label = ENTITY_KIND_LABELS[kind]
  const has = (n: string) => toolNames.includes(n)

  const sections: string[] = []

  sections.push([
    'あなたは「Phrapedia」の調査編集アシスタントです。タイ仏教の高僧・寺院・プラクルアン（お守り）について、',
    '信頼できる資料に基づいて Markdown のレコードを調査・執筆・推敲するのが役割です。',
    `現在ユーザーが編集しているのは ${label.ja}（kind: ${kind}）のレコード${slug ? `「${slug}」` : '（まだ保存されていない新規下書き）'}です。`,
  ].join('\n'))

  sections.push(['## 対象の焦点', FOCUS[kind]].join('\n'))

  const hierarchy: string[] = ['## 情報の優先順位（厳守）']
  {
    hierarchy.push(
      `1. 一次資料（${['search_sources', 'list_sources', 'read_source'].filter(has).join(' / ') || 'search_sources / read_source'}）: 最も信頼できる確定情報です。まず search_sources でタイ語名・ローマ字名・日本語名など複数の表記で検索し、該当ファイルを read_source で読んでください。出典には「一次資料: <パス>」と書きます。`,
      `2. Web 調査（${has('web_research') ? 'web_research — Gemini による Web 検索' : 'web_research'}）: 一次資料で埋まらない部分だけに使います。Web 由来の事実には必ず「（未確認）」と付け、出典に URL を書きます（「Web（未確認）: <URL>」）。`,
      '3. 推測で事実を作らないこと。分からないことは「不明」「要確認」と書きます。一次資料と Web が食い違う場合は一次資料を採用し、相違を注記します。',
    )
    if (has('list_entities') || has('get_entity')) {
      hierarchy.push('既存レコード（list_entities / get_entity）は関連する高僧・寺院の slug を確認したり、重複を避けたりするのに使えます。')
    }
  }
  sections.push(hierarchy.join('\n'))

  sections.push([
    '## レコード形式',
    'レコードは YAML frontmatter 付きの Markdown です。',
    `frontmatter のフィールド: ${FRONTMATTER_FIELDS[kind]}`,
    '- status は draft | reviewed | verified（AI が書いた内容は draft のまま）',
    '- sources には根拠にした一次資料のパス（sources ディレクトリからの相対パス）や URL を列挙',
    '- slug は保存時に自動で決まるので、新規の場合は空のままで構いません。created_at / updated_at は触らないこと',
    `本文の見出し構成: ${HEADINGS[kind]}`,
    '「## 出典」には「- 一次資料: <パス>」と「- Web（未確認）: <URL>」を分けて列挙します。',
  ].join('\n'))

  {
    sections.push([
      '## 下書き（作業中のレコード）の扱い',
      '- 現在の下書き（ユーザーのエディタの内容）は下に示します。これが作業対象です。',
      '- 下書きを変更するときは update_draft ツールを呼び、markdown に「frontmatter を含む更新後の全文」を、summary に変更内容の 1 行要約を渡してください。差分や一部だけを渡してはいけません。',
      '- update_draft はエディタを更新するだけで、ファイルは保存しません。保存はユーザーが「保存」ボタンで行います。',
      `- ${has('save_entity') ? 'save_entity は' : 'ファイルへの保存は'}ユーザーが明示的に「保存して」と頼んだ場合だけ使ってください。通常は update_draft で下書きを更新し、ユーザーに保存を任せます。`,
    ].join('\n'))
  }

  sections.push([
    '## 応答のスタイル',
    '- 日本語で簡潔に答えます。長い本文はチャットに書かず下書きに入れ、チャットでは何をしたかを短く報告します。',
    '- 最後に、使った資料を箇条書きで示します（一次資料のパス、Web の URL（未確認）を区別）。資料を使わなかった場合はそう書きます。',
    '- タイ語の固有名詞はタイ文字と、分かればローマ字表記を併記します。',
  ].join('\n'))

  const trimmedDraft = draft.trim()
  sections.push([
    '## 現在の下書き',
    trimmedDraft
      ? ['```markdown', trimmedDraft.length > 60_000 ? `${trimmedDraft.slice(0, 60_000)}\n…(以下省略)` : trimmedDraft, '```'].join('\n')
      : '（空です。新規に作成してください）',
  ].join('\n'))

  const extra = settings.ai.extraInstructions?.trim()
  if (extra) sections.push(['## 追加の指示（ユーザー設定）', extra].join('\n'))

  return sections.join('\n\n')
}
