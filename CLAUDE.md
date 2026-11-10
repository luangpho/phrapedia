# Phrapedia — AI コーディングエージェント向けガイド

タイの高僧・寺院（将来: プラクルアン）の知識ベース。Markdown ファイルが正本。デスクトップ専用（Rust + Tauri v2）。詳細な契約は `docs/ARCHITECTURE.md` と `docs/NATIVE_API.md`。

## スタック
Rust + Tauri v2（バックエンド全部。実行時に Node / Nitro / PGlite は使わない）/ Nuxt 4 を **SPA 静的生成専用**（`ssr:false`、TS strict）/ Nuxt UI v4 / Tailwind v4 / SQLite（`rusqlite` bundled、FTS5）/ Claude Code CLI `claude -p`（チャット）/ Antigravity CLI `agy`（Web 調査。旧 Gemini CLI も選択可）/ git CLI（同期）/ MCP は `rmcp` による stdio（`phrapedia mcp`）。node / npm は **ビルド時のみ**必要。

## コマンド
```bash
npm ci
npm run dev         # nuxt dev --port 1420（Tauri の devUrl。単体のブラウザでは Tauri command が無いので機能しない）
npm run generate    # SPA を .output-frontend に静的生成（tauri build の beforeBuildCommand）
npm run typecheck   # nuxt typecheck — 変更後は必ず通す（vue-router/volar の解決警告は出るが型エラーではない）
npm test            # vitest（tests/**/*.test.ts。api / chat-events / dialog / slug / updater / update-cask）
npm run tauri:dev   # デスクトップアプリ開発起動（Rust 必要）
npm run tauri:build # デスクトップアプリのビルド（deb / rpm / AppImage / macOS dmg。src-tauri/tauri.bundle.conf.json を重ねる）
cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings && cargo test   # Rust 側。変更後は必ず通す
```
`build` / `preview` / `start` / `start:render` と `render.yaml` / `scripts/render-start.mjs` は旧 web 版（Render）の残置。この構成では動かない（`.output/server` は生成されない）。削除は未判断。

## 実行形
- `src-tauri/src/main.rs`: 先頭引数が `mcp` なら Tauri を初期化せず stdio MCP サーバ（`phrapedia mcp --config-dir <dir> --data-dir <dir>`）。それ以外は GUI。
- フロントは `app/utils/api.ts` だけがバックエンド窓口（`invoke` の薄いラッパと `chatSend` の Channel）。契約は `docs/NATIVE_API.md`。command の追加・変更は api.ts・`commands/`・NATIVE_API.md・`tests/api.test.ts` を揃える。
- 設定は 2 パス: **データセットパス**（読み出し元）と **書き出し先パス**（保存先・git 同期対象）。app config dir の `settings.json` に保存。環境変数ではなく設定画面で指定。不正パスでもアプリは起動し、該当 command が `path_unusable` で reject。
- 状態 DB（チャット履歴・一次資料の FTS5 index・Web 調査キャッシュ）は `<app data dir>/state/phrapedia.sqlite`。**旧 PGlite のデータは移行しない**。

## 配布
- release は `scripts/release.sh <version>` の 1 コマンド（版上げ commit・push・tag・workflow 待ちまで）。タグ `v*` の push で `.github/workflows/release.yml` が macOS（aarch64 / x64）を tauri-action でビルドし、GitHub Release に `.dmg` を添付。署名は ad-hoc が既定（`APPLE_*` secret があれば署名・notarization）。
- アプリ内更新（tauri updater）は `src-tauri/tauri.updater.conf.json` の pubkey と secret `TAURI_SIGNING_PRIVATE_KEY` が両方ある時だけ有効（`.app.tar.gz` / `.sig` / `latest.json` を添付）。フロントは Tauri command `updater_enabled`（`plugins.updater` の有無。plugin の条件付き登録と同じ判定）が true の時だけサイドバーの更新アイコン（`UpdateButton.vue`）を出す。
- 更新の起動はアイコン押下のみ（`app/composables/useUpdater.ts` の状態機械）。起動時と 6 時間ごとの `check()` は静かに行い、自動ダイアログは出さない。確認は `confirmDialog`（`window.confirm` は WKWebView で動かない）。権限不足の失敗は `brew upgrade --cask phrapedia` を案内。
- Homebrew: `packaging/homebrew/Casks/phrapedia.rb`（Node への依存なし）。`packaging/homebrew/update-cask.mjs` が version と sha256 を書き換え、`HOMEBREW_TAP_SSH_KEY`（推奨）か `HOMEBREW_TAP_TOKEN` があれば `luangpho/homebrew-tap` へ push。
- 成果物名は `Phrapedia_<version>_{aarch64,x64}.dmg`。`productName` / `version` を変えたら cask の `url` とワークフローのファイル名を合わせる。version は `package.json` / `package-lock.json` / `src-tauri/Cargo.toml` / `src-tauri/Cargo.lock` / `src-tauri/tauri.conf.json` で揃える（`release.sh` が行う）。
- 手順の詳細は README の「リリース手順」。

## どこに何があるか
- `data/<kind>/<slug>.md` — レコード（YAML frontmatter + 本文）。kind = monks | temples | amulets（将来）
- `sources/` — 一次資料（データセットパス配下）
- `shared/types/` — app 共通型、`shared/utils/slug.ts`
- `app/` — Nuxt SPA（pages, components (`chat/`, `entity/`, `markdown/`, `sync/`, `desktop/`), composables, `utils/api.ts`）
- `src-tauri/src/core/` — Tauri 非依存の中核: paths / path_settings / app_settings / frontmatter / entities / sources / store(SQLite) / git / research / cli
- `src-tauri/src/commands/` — `#[tauri::command]` 層（`chat.rs` が `chat_send` / `chat_abort`）
- `src-tauri/src/chat/` — `claude -p` の実行（stream / claude / prompt / history / turn）
- `src-tauri/src/mcp/` — stdio MCP（`tools.rs` が 11 ツール・resource・prompt、`server.rs` が rmcp アダプタ）
- `src-tauri/examples/chat_smoke.rs` — 実 `claude` でチャットを往復・中断・resume させる手動確認
- `src-tauri/capabilities/default.json` — WebView の権限（必要最小。増やす時は理由を残す）
- `docs/NATIVE_API.md` — command / イベント / MCP 起動形の契約、`docs/reports/` — 作業報告（過去の記録）

## ルール
- **レコードは Markdown**。DB に正本を置かない（SQLite は履歴・インデックス・キャッシュのみ）。書き込みは core の entities 経由で frontmatter を正規化（自前実装。既存データを開いて保存しても `updated_at` 以外の差分を出さない）。
- **一次資料は読み取り専用**。アプリ・ツールから `sources/` に書き込まない。パスは `core::paths` の安全 join で検証（`..`・symlink 脱出の拒否）。
- 一次資料 > Web 調査。Web 由来は「未確認」と明記し、`## 出典` で一次資料と Web を分ける。
- **チャット AI は Claude 固定**。実行経路は Claude Code CLI の headless 実行（`claude -p`、ユーザの `claude` ログインで動く。API キーはアプリで扱わない）。Gemini（Antigravity CLI）は `web_research` ツールのインターネット検索にのみ使う。チャットの代替プロバイダは追加しない。
- チャットの `claude -p` には自バイナリの `phrapedia mcp`（stdio）だけを `--mcp-config` で渡し、組み込みツールは無効（`--tools ""`）、`sync_data` は許可しない。system prompt と MCP 設定は 0600 の一時ファイルで渡し、argv（`ps`）に中身を出さない。子は専用 process group で起動し、中断・timeout・終了後に group ごと回収する。下書き更新は MCP ツール `update_draft`（アプリ内チャット専用）の `tool_use` を見て `draft` イベントにする。
- **UI は日本語**。Nuxt UI のトークン（`text-muted`, `bg-elevated` 等）を使いダークモードを壊さない。アイコンボタンには `aria-label`。
- 外部 CLI（`claude` / `agy` / `gemini` / `git`）は `core::cli` の解決（継承 PATH → login shell の PATH → 固定候補）とタイムアウト付き実行を使う。
- Git 同期でステージするのはデータフォルダのみ。
- 秘密情報（updater の秘密鍵、`GEMINI_API_KEY`）は commit しない。updater の pubkey・identifier `jp.grrow.phrapedia`・productName `Phrapedia`・成果物名は変えない。
