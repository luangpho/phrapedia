# Phrapedia アーキテクチャ

タイの高僧・寺院・プラクルアン（お守り）の情報を、**Markdown ファイルを正本**として蓄積するローカルファースト型の知識ベース。

## 技術スタック

| 層 | 採用技術 |
|---|---|
| フレームワーク | Nuxt 4（SSR）, TypeScript strict |
| UI | Nuxt UI v4（Dashboard / Chat コンポーネント）, Tailwind CSS v4, `@tailwindcss/typography` |
| MCP | `@nuxtjs/mcp-toolkit` — `server/mcp/tools/*.ts` が `/mcp`（Streamable HTTP）として公開され、同じ定義をアプリ内チャットAIも使う |
| アプリ状態 DB | PGlite（埋め込み Postgres, `.phrapedia/pglite/`）— 設定・チャット履歴・一次資料インデックス・Web調査キャッシュ |
| 一次情報 | `sources/`（設定で変更可）配下の Markdown / 画像。**最優先・確定情報**として扱う |
| Web 調査 | Google の CLI を `child_process` で呼び出す。既定は **Antigravity CLI**（`agy --output-format json -p ...`。2026-06 に無料枠/Google One の Gemini CLI を置き換え）、旧 **Gemini CLI**（`gemini`、プロンプトは stdin）も選択可。結果は常に「未確認」扱い |
| チャット AI | Anthropic Claude 固定（ツール呼び出しループ, SSE ストリーミング）。Gemini は `web_research` ツール経由のインターネット検索にのみ関与し、チャット自体は担わない |
| 同期 | `simple-git` で `data/` をコミット → `pull --rebase` → `push`（GitHub） |

## ディレクトリ

```
app/                      Nuxt アプリ（pages, components, composables, layouts）
  components/chat/Panel.vue      <ChatPanel> AI チャット窓
  components/markdown/*.vue      Markdown 表示・編集
  components/entity/*.vue        サイドバー一覧など
  composables/useEntities.ts / useChat.ts / useSettings.ts / useSync.ts
shared/types/             app / server 共通の型（EntityKind, EntityRecord, AppSettings, ChatStreamEvent, SyncStatus ...）
shared/utils/slug.ts      slug 生成
server/utils/             paths, db(PGlite), settings, frontmatter, entities, sources, gemini, git, mcp-bridge
server/mcp/tools/         MCP ツール（list_entities, get_entity, save_entity, entity_template, list_sources,
                          search_sources, read_source, web_research, sync_status, sync_data）
server/mcp/resources/     phrapedia://{kind}/{slug} リソース
server/mcp/prompts/       research_record プロンプト
server/api/               REST API（下記）
data/<kind>/<slug>.md     レコード（正本, git 管理）  kind = monks | temples | amulets(将来)
sources/                  一次資料（既定パス。設定画面で任意パスに変更可）
.phrapedia/               ローカル状態（PGlite）— git 管理外
```

## レコード形式（Markdown + YAML frontmatter）

```markdown
---
slug: luang-pho-to
kind: monks
name: ルアンポー・トー
name_th: หลวงพ่อโต
name_en: Luang Pho To
aliases: [ソムデット・トー]
temple: wat-rakhang
province: バンコク
born: "1788"
died: "1872"
status: draft        # draft | reviewed | verified
sources: [notes/somdej-to.md, images/somdej-to-1.jpg]
tags: [ソムデット]
created_at: 2026-11-05T00:00:00.000Z
updated_at: 2026-11-05T00:00:00.000Z
---

# ルアンポー・トー

## 概要
...
## 出典
- 一次資料: notes/somdej-to.md
- Web（未確認）: https://...
```

`server/utils/entities.ts` が slug / kind / name / created_at / updated_at を正規化して書き戻す。未知のキーは保持される。

## REST API

| Method | Path | 説明 |
|---|---|---|
| GET | `/api/entities/:kind` | 一覧 `EntitySummary[]` |
| GET | `/api/entities/:kind/template?name=` | 新規用テンプレート `{ raw }` |
| POST | `/api/entities/:kind` | 作成 `{ raw, message? }` → `{ record, commit }`（slug は frontmatter から導出） |
| GET | `/api/entities/:kind/:slug` | `EntityRecord` |
| PUT | `/api/entities/:kind/:slug` | 更新 `{ raw, message? }` → `{ record, commit }` |
| DELETE | `/api/entities/:kind/:slug` | 削除 |
| GET | `/api/sources?dir=` | 一次資料一覧 `SourceFile[]` |
| GET | `/api/sources/search?q=&limit=` | 検索 `SourceSearchHit[]` |
| GET | `/api/sources/file/<path>` / `?raw=1` | 内容（JSON）/ バイナリ配信 |
| POST | `/api/sources/reindex` | インデックス再構築 |
| POST | `/api/research` | Gemini CLI で Web 調査 `{ query, context?, noCache? }` |
| GET/PUT | `/api/settings` | 設定（`SettingsView` / `Partial<AppSettings>`） |
| GET | `/api/sync/status` | `SyncStatus` |
| POST | `/api/sync/commit` `/pull` `/push` `/sync` | Git 操作 → `SyncResult` |
| POST | `/api/chat` | チャット（SSE: `ChatStreamEvent`） |
| GET | `/api/chat/sessions?kind=&slug=` | セッション一覧 |
| GET/DELETE | `/api/chat/sessions/:id` | メッセージ取得 / 削除 |
| PATCH | `/api/chat/sessions/:id` | `{ slug?, title? }` — 下書き用セッションを保存後のレコードへ紐付け直す |
| GET | `/api/health` | 動作確認 |
| POST | `/mcp` | MCP エンドポイント（Streamable HTTP, stateless） |

## チャット AI の設計

1. ブラウザは `POST /api/chat` に `{ sessionId?, kind, slug, message, draft }` を送る。`draft` はエディタの現在の Markdown 全文。
2. サーバは `createMcpBridge()` で MCP ツールをインプロセス接続し、ツール定義（JSON Schema）を LLM に渡す。
   さらにチャット専用ツール `update_draft({ markdown, summary })` を追加する。LLM がこれを呼ぶとサーバは
   `{ type: 'draft', markdown }` イベントを流し、ブラウザのエディタが更新される（ファイルは保存しない。保存はユーザーの操作）。
3. システムプロンプトの原則: 一次資料（search_sources / read_source）を最優先 → 不足分のみ web_research（Gemini）→
   Web 由来は「未確認」と明記 → 出典セクションに一次資料パスと URL を分けて書く → 高僧の追加では高僧中心、寺院の追加では寺院中心。
4. ストリーミングは h3 `createEventStream(event)` の SSE。イベント型は `shared/types/chat.ts` の `ChatStreamEvent`。
5. 履歴は PGlite（chat_sessions / chat_messages）に保存（`server/utils/chat-store.ts`: createChatSession / listChatSessions / appendChatMessage ...）。セッションは (kind, slug) に紐づき、slug が null の下書きセッションはブラウザの localStorage で記憶され、保存後に PATCH で slug へ付け替えられる。
6. 役割分担: Claude が会話・一次資料の読解・下書きの執筆を担い、Web からの情報収集だけを `web_research`（Gemini）に委ねる。Gemini の出力は検索結果の要約＋出典 URL で、レコード本文は Claude が書く。

## MCP クライアントからの利用

```jsonc
// Claude Code: .mcp.json / Antigravity CLI: `agy mcp add` または ~/.gemini/config/mcp_config.json / Gemini CLI: ~/.gemini/settings.json
{ "mcpServers": { "phrapedia": { "httpUrl": "http://localhost:3000/mcp" } } }
```
