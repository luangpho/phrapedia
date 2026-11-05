# Phrapedia — AI コーディングエージェント向けガイド

タイの高僧・寺院（将来: プラクルアン）の知識ベース。Markdown ファイルが正本。詳細な契約は `docs/ARCHITECTURE.md`。

## スタック
Nuxt 4 (SSR, TS strict) / Nuxt UI v4 / Tailwind v4 / `@nuxtjs/mcp-toolkit`（`/mcp`）/ PGlite（`.phrapedia/`）/ Anthropic SDK（チャット）/ Antigravity CLI `agy`（Web 調査。旧 Gemini CLI も選択可）/ simple-git。

## コマンド
```bash
npm run dev         # http://localhost:3000
npm run build
npm run typecheck   # nuxt typecheck — 変更後は必ず通す
npm test            # vitest (tests/**/*.test.ts, tests/setup.ts が Nitro のグローバルをスタブ)
npm run smoke       # build 後に REST + MCP を疎通確認
```

## どこに何があるか
- `data/<kind>/<slug>.md` — レコード（YAML frontmatter + 本文）。kind = monks | temples | amulets（将来）
- `sources/` — 一次資料（設定で変更可）
- `shared/types/` — app/server 共通型、`shared/utils/slug.ts`
- `server/utils/` — paths, db, settings, frontmatter, entities, sources, gemini, git, agent/（チャット）
- `server/mcp/{tools,resources,prompts}/` — MCP 定義（アプリ内チャットも同じツールを使う）
- `server/api/` — REST API（一覧は ARCHITECTURE.md）
- `app/` — pages, components (`chat/`, `entity/`, `markdown/`, `sync/`), composables

## ルール
- **レコードは Markdown**。DB に正本を置かない（PGlite は設定・履歴・インデックス・キャッシュのみ）。書き込みは `writeEntity()` 経由で frontmatter を正規化。
- **一次資料は読み取り専用**。アプリ・ツールから `sources/` に書き込まない。パスは `safeJoin()` で検証。
- 一次資料 > Web 調査。Web 由来は「未確認」と明記し、`## 出典` で一次資料と Web を分ける。
- **チャット AI は Claude 固定**。Gemini（Antigravity CLI）は `web_research` ツールのインターネット検索にのみ使う。チャットの代替プロバイダは追加しない。
- **UI は日本語**。Nuxt UI のトークン（`text-muted`, `bg-elevated` 等）を使いダークモードを壊さない。アイコンボタンには `aria-label`。
- サーバ側は Nitro の自動インポート（`createError`, `useDb`, `getSettings` 等）を使う。新しいグローバルをテストで使う場合は `tests/setup.ts` にスタブを追加。
- Git 同期でステージするのはデータフォルダのみ。
