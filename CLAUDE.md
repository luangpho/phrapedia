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
npm run tauri:dev   # デスクトップアプリ開発起動（npm run build && tauri dev。Rust 必要）
npm run tauri:build # デスクトップアプリのビルド（src-tauri/tauri.bundle.conf.json で .output/server を同梱）
```

## デスクトップクライアント（Tauri）と設定項目
- `src-tauri/` が Nuxt の SPA（`PHRAPEDIA_TAURI=1 nuxt generate` → `.output-frontend`）を WebView に載せ、`.output/server` を Node sidecar（`127.0.0.1` の空きポート）で起動する。CLI（`npm run dev` / `npm start`）と Render は従来どおり。
- 設定は 2 パス: **データセットパス**（読み出し元。env `PHRAPEDIA_DATASET_PATH`）と **書き出し先パス**（保存先・git 同期対象。env `PHRAPEDIA_EXPORT_PATH`）。app config dir の `settings.json` に保存。受け口は `server/utils/paths.ts`。不正パスは 409。詳細は `docs/ARCHITECTURE.md`。

## 配布
- タグ `v*` の push で `.github/workflows/release.yml` が macOS（aarch64 / x64）を tauri-action でビルドし、GitHub Release に `.dmg` / `.app.tar.gz` を添付。`APPLE_*` secret があれば署名・notarization、無ければスキップ。
- Homebrew: `packaging/homebrew/Casks/phrapedia.rb`（`depends_on formula: "node"`）。`packaging/homebrew/update-cask.mjs` が version と sha256 を書き換え、`HOMEBREW_TAP_TOKEN` があれば `luangpho/homebrew-tap` へ push。
- 成果物名は `Phrapedia_<version>_{aarch64,x64}.dmg`。`productName` / `version` を変えたら cask の `url` とワークフローのファイル名を合わせる。version は `package.json` / `Cargo.toml` / `tauri.conf.json` で揃える。
- 手順の詳細は README の「リリース手順」。

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
