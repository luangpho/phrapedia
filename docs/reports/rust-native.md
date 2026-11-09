# Rust + Tauri ネイティブ化

branch: `feat/rust-native`。実行時の Node / Nitro / PGlite を無くし、WebView の SPA + Rust の Tauri command だけで動かす。
この文書は worker ごとの節に分かれる。**core 節**は core worker が書いた（Rust core と command 層）。chat / frontend / integrate-test / review の各節は担当 worker が追記する。

契約（command 名・引数・戻り値・エラー型・`ChatStreamEvent`・MCP 起動形）は [docs/NATIVE_API.md](../NATIVE_API.md)。

## 全体サマリ

- 実行時の Node / Nitro / PGlite を撤去し、Rust（Tauri v2 の command 層 + core + claude -p の Rust 実装）と WebView の Nuxt SPA だけで動く構成にした。外部クライアント向けは HTTP `/mcp` の代わりに `phrapedia mcp`（stdio）。
- 既定 branch の PR #5（チャット claude -p 化）・PR #6（更新アイコン）・0.1.3 版上げを merge で取り込み済み（version 0.1.3）。
- Linux/204 で `npm test` 83 件、`cargo test` 143+3 件、clippy `-D warnings`、`tauri:build`（deb/rpm/AppImage + updater 署名）、xvfb 30 秒起動、node 無し PATH 起動、`phrapedia mcp` stdio、実 claude でのチャット往復を確認。詳細は integrate 節。
- 非互換: 旧 PGlite データは移行しない／HTTP `/mcp` 廃止／ブラウザ版（Web 版）は旧 branch のまま。

## core 節

### 構成

- `src-tauri/src/core/`: GUI と `phrapedia mcp`（chat worker 担当）の両方から使う lib module。Tauri 非依存（`Core` は `AppDirs` だけ受け取る）。
  - `paths.rs` / `path_settings.rs`: `safeJoin` 相当（`..`・symlink 脱出の拒否、先頭 `/` の許容）と `settings.json`（`PHRAPEDIA_DATASET_PATH` / `PHRAPEDIA_EXPORT_PATH`。app config dir、絶対パス必須）。旧実装と同じファイル・同じキーなので既存設定はそのまま読める。一次資料（`sources/`）は読み取り専用（書き込み API を持たない）。
  - `app_settings.rs`: AppSettings（ai / gemini / git / dataDir 等）。app config dir の `app-settings.json`。**上書き分だけ**保存し、既定値は読む時にマージ（`server/utils/settings.ts` の移植）。型違いの値は既定に戻す。旧 Anthropic モデル名の移行あり。
  - `frontmatter.rs` / `entities.rs` / `slug.rs`: Markdown が正本。list / get / create / update / delete / template。
  - `sources.rs`: 一覧・読み出し（テキストは切り詰め、画像は base64 と asset 用バイト列）・再 index（増分。サイズ + mtime が変わったファイルだけ再読込）・全文検索。
  - `store.rs`: SQLite。チャットのセッション / メッセージ / `claude_session_id`、source の FTS5 index、web 調査キャッシュ。
  - `git.rs`: git CLI を spawn。`status / commit / pull / push / sync`。stage は dataDir だけ。
  - `research.rs`: `agy` / `gemini` CLI の spawn（timeout・SQLite キャッシュ・JSON 封筒の展開・画像の作業 dir 経由の受け渡し）。
  - `cli.rs`: 外部 CLI の解決とタイムアウト付き実行。
- `src-tauri/src/commands/`: `#[tauri::command]` 層（REST と 1 対 1）。エラーは日本語 message 付きの構造化エラー（`error.rs`）。
- `lib.rs`: plugin 登録（dialog / process / 条件付き updater）、`Core` を state に載せる、command 登録。`updater_enabled`（update-icon 由来）もここ。

### 設計判断

- **store の置き場**: app data dir（`<app data dir>/state/phrapedia.sqlite`、WAL + busy_timeout）。従来の `.phrapedia/`（書き出し先の中）は採らない。書き出し先は git 同期の対象で、DB が混ざるとコミット対象の事故になるため。GUI と `phrapedia mcp`（別プロセス）が同じ DB を開く前提で WAL にしてある。
- **設定の置き場**: `settings.json`（2 パス）と `app-settings.json` を同じ app config dir に置く。JSON を選んだのは、既存の設定ファイルと同じ扱いで手で直せるため。DB は検索 index とチャット用に限る。
- **frontmatter の出力は自前実装**: 既存データを開いて保存しても差分を出さないため。旧実装は npm の `yaml`（`lineWidth: 0`、plain 文字列、配列は key の下にインデント）で、その出力と一致させる必要がある。YAML 出力用の crate はどれもこの整形（引用符の選び方、`|` / `|-` / `|+`、空 map `{}`）を再現しないので、出力は自前で書き、**実際の `yaml` パッケージで出力を突き合わせた**（引用符の選択: `say "hi"` や `both ' and "` はそのまま plain、`- x` / `# c` / `tail:` は `"…"`）。
- **frontmatter の入力は `serde_yaml_ng`**: `serde_yaml` は archived で deprecated。`serde_yaml_ng` はその保守されている fork。日付らしい値は文字列のまま保つ（旧実装の `normaliseDates` 相当）。キー順は `serde_json` の `preserve_order` で保つ。
- **全文検索**: SQLite FTS5 + `trigram` tokenizer。タイ語・日本語は単語区切りが無く、unicode61 では使えないため。trigram は 3 文字未満の語に効かないので、**全項目が 3 文字以上の時だけ FTS、それ以外は全件スキャン**に切り替える。スコアは旧実装と同じ（語ごとに本文一致 +2、パス一致 +1）。FTS の構文文字（`"` `OR` `NOT` `(`）は語を引用符で包んで無効化。
- **git は CLI の spawn**: simple-git も CLI のラッパーなので挙動が揃う。branch / remote 名は `--` 始まりを拒否して引数注入を防ぐ。URL の認証情報は出力から伏せる。dataDir が書き出し先の外なら旧実装と同じく拒否。
- **外部 CLI の解決**: 継承 PATH → login shell の PATH（`$SHELL -lc`、4 秒 timeout、プロセス内で 1 回だけ）→ 固定候補（`/opt/homebrew/bin` `/usr/local/bin` `$HOME/.local/bin` `~/.claude/local` `~/.npm-global/bin` `~/.cargo/bin` `~/.bun/bin`）の順で結合。設定の command が `/` を含めばそのまま使う（絶対パス可）。子プロセスは新しい process group で起動し、timeout 時は group ごと SIGTERM → 2 秒後 SIGKILL。
- **F の整理**: `sidecar.rs`（sidecar 起動・ポート待ち・parent-watch・Node 探索）を削除。`tauri.bundle.conf.json` から `.output/server` の同梱を外した。cask から `depends_on formula: "node"` を削除（`preflight_steps` と `auto_updates` は維持）。capabilities は `core:default / dialog:allow-open / dialog:allow-message / updater:default / process:allow-restart` のみ（`core:default` 以外は update-icon と同じ）。

### crate 選定

| crate | 用途 | 理由 |
|---|---|---|
| `serde_yaml_ng` 0.10 | YAML 入力 | `serde_yaml` は archived。保守されている fork |
| `rusqlite` 0.37（`bundled`） | store / FTS5 | bundled は FTS5 付きの SQLite を同梱。実行環境の SQLite に依存しない |
| `serde_json`（`preserve_order`） | JSON / キー順 | frontmatter のキー順維持に必要 |
| `sha2` | 調査キャッシュのキー | web_research の入力 hash |
| `base64` | 画像の返却 | 旧 API の `base64` フィールド互換（sources / research） |
| `uuid`（v4） | id 採番 | チャットセッション id、research の作業 dir |
| `chrono`（`clock`, `std`） | 日時 | `created_at` / `updated_at` / `indexed_at` の ISO 文字列 |
| `unicode-normalization` | slug | `shared/utils/slug.ts` の正規化 |
| `libc`（unix） | process group の kill | 追加の依存は最小 |

### 検証

- `cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings && cargo test`: すべて通過。テスト 90 件（core 86 件 + updater 設定 4 件）。
- テストの範囲: frontmatter の往復（タイ語・日本語・配列・入れ子・引用符・数字や真偽に見える文字列・複数行・日付・BOM / CRLF・不正 YAML）、slug、パス検証（`..`・symlink 脱出・内側 symlink）、entities の CRUD（一時 dir）、sources の一覧 / 読み出し / 増分 index / 検索（タイ語・日本語・英語、短語のスキャン経路、FTS 構文文字）、git（一時 repo で status / commit / pull / push / sync、dataDir 外の拒否、危険なブランチ名の拒否、認証情報の伏せ字）、設定の既定値・深いマージ・型違いのフォールバック・移行、CLI の解決と timeout 強制終了、research の引数組み立て・封筒展開・画像の作業 dir の後始末。
- **リポジトリ同梱の `data/` を読んで保存する確認**（`entities::tests::saving_a_bundled_record_changes_only_updated_at`）: monks / temples の全レコードを一時 dataset に複製し、`get_entity` → `write_entity`（`must_exist`）して元ファイルと行単位で比較。差分は **`updated_at:` の 1 行だけ**。これは旧実装も保存のたびに `updated_at` を更新するためで、frontmatter の整形差は無い。`amulets/` は README のみでレコード無し。
- 修正した不具合: login shell の PATH 取得が `OnceLock` の初期化中に `child_path()` を再帰的に呼んでデッドロックしていた（`cargo test` が無出力で止まる症状）。取得側は継承 PATH だけで子プロセスを起動するようにして解消。

### 旧実装との非互換 / 注意

- **PGlite のデータは移行しない**（チャット履歴・source index・調査キャッシュ）。source index と調査キャッシュは再構築される。チャット履歴は失われる（README への明記は integrate-test の担当）。
- REST / SSE / sidecar の `baseUrl` は無い。`sidecar_info` → `app_info`、`save_path_settings` に **サーバ再起動は無い**（保存 = 即反映、`restartError` は廃止）。置換対応表は NATIVE_API.md。
- ファイルの最後の複数行ブロック scalar は、frontmatter 終端の直前の改行が区切りに食われて末尾改行が落ちる。旧実装（`yaml` + 正規表現による分割）も同じ挙動で、往復で差分は出ない。
- 検索は 3 文字未満の語で全件スキャンになる（件数が多い dataset では遅くなる可能性。実測していない）。
- 短い語・FTS の順位は SQLite の `rank` ではなく、旧実装のスコア式を Rust で再計算している。同点はパス昇順。

### 未検証 / 未実施

- macOS 実機での GUI 起動 PATH（login shell 取得と固定候補で `claude` / `agy` / `gemini` / `git` が見つかるか）。Linux で単体テストのみ。
- 実機の `agy` / `gemini` / `claude` を実際に spawn した web_research（引数組み立てと封筒展開はスタブ・単体テストのみ）。
- `npm run tauri:build`（frontend が API 層に置換するまで旧 Nuxt のままなので対象外）。
- 大きな dataset での index / 検索の性能。
- Windows（`#[cfg(unix)]` の process group kill 以外は未確認）。

## frontend 節（Nuxt を SPA 静的生成専用に）

### 置換方針

- `app/utils/api.ts` が唯一のバックエンド窓口。`api.<command>()`（`invoke` の薄いラッパ）、`chatSend()`（`Channel` を包み `ChatStreamEvent` をそのまま渡す）、`sourceBlobUrl()`（`read_source_bytes` → Blob URL、パス単位でキャッシュ）を提供。Nuxt の auto-import に依存しないので vitest（Node）でそのままテストできる。
- エラーは `AppError` JSON を `ApiError`（`code` / `message` / `detail` + `statusCode`: not_found=404, invalid_argument=400, conflict・path_unusable=409）に変換。`errorMessage()`（`utils/entity.ts`）は `message` を取り出す。
- ブラウザ単体（`__TAURI_INTERNALS__` 無し）は import 時に例外を出さず、各 call が `code: 'desktop_only'` の「この機能はデスクトップアプリ専用です。…」で reject。各画面は既存のエラー表示にそのまま出る。
- データ取得は `useFetch` → `useAsyncData`（key は従来のまま: `entities:<kind>` / `entity:<kind>:<slug>` / `sync-status` / `settings` / `sources` ほか）。`refreshNuxtData` / `clearNuxtData` の呼び出しは変更なし。
- チャット: `useChat` は `chat_send` に `turnId`（クライアント採番 UUID）+ Channel を渡す。`stop()` は `chat_abort` を呼び、バックエンドの `done`（停止メッセージ）を受けて終了。`reset()` / 記録切替 / unmount は `chat_abort` + Channel の handler を無効化（遅れて届くイベントを無視）。SSE パーサは削除。`applyChatStreamEvent` は不変。
- 一次資料の画像 / PDF: markdown 内の相対パスは `#phra-src/<path>` のマーカー href に書き換え、`MarkdownView` が描画後に `read_source_bytes` → Blob URL へ差し替える（asset protocol は不使用）。`SourcesPreview` も同様。
- デスクトップ層: `sidecar_info` / `setApiBase` / `window.fetch` の差し替え / `restart_sidecar` を廃止（`api-base.ts` 削除）。プラグインは `app_info` を読むだけ。`DesktopBanner` の「再試行」は `app_info` の再読込、`PathsCard` は「保存して再起動」→「保存」（トーストも更新）。
- 画面: ホームの「MCP エンドポイント」カードは HTTP `/mcp` の URL 表示から **stdio 設定例（command=実行バイナリの絶対パス、args=["mcp","--config-dir",…,"--data-dir",…]）のコピー表示**に書き換え（デスクトップ時のみ表示）。値は `app_info` に追加した `exePath` / `configDir` / `dataDir`（core の `AppInfo` への加算変更 + NATIVE_API.md 追記）から作る。それ以外の画面・文言は維持。
- `shared/types`: `SettingsView.resolved.projectRoot` → `datasetRoot` / `exportRoot`、`ChatRequest.turnId` を追加。

### 削除物

- `server/**` 全部（api / middleware / plugins / utils / mcp）、`scripts/smoke.mjs`、`.env.example`、`app/utils/api-base.ts`。
- 依存: `@electric-sql/pglite` `@nuxtjs/mcp-toolkit` `simple-git` `gray-matter` `yaml` `zod`、devDependency `ai`（`@nuxt/ui` の optional peer で app 側は未使用）。lockfile は `npm install` で再生成。
- scripts: `smoke` `generate:tauri` `dev:tauri` を廃止。`build` `preview` `start` `start:render` は **未変更で残置**（render 関連は未変更。`render.yaml` / `scripts/render-start.mjs` も削除せず、存続は利用者が merge 時に判断。web 版は旧 branch のまま。新 nuxt.config では `start` が参照する `.output/server` は生成されないため、この branch では動かない）。`dev` = `nuxt dev --port 1420`、`generate` = `nuxt generate`、`tauri:dev` = `tauri dev`、`tauri:build` は従来どおり。
- `nuxt.config.ts`: `ssr:false` 固定、`PHRAPEDIA_TAURI` 分岐 / `mcp` / `runtimeConfig` のサーバ項目 / nitro externals / vite の pglite 設定を削除。出力先 `.output-frontend` は維持（`frontendDist` 不変）。`@nuxt/icon` は `provider:'none'`・`serverBundle:false`・`fallbackToApi:false`（client bundle のみ。旧 sidecar の icon API には依存しない）。
- `tauri.conf.json`: `beforeDevCommand=npm run dev` / `beforeBuildCommand=npm run generate`、CSP から `http://127.0.0.1:*` を除き `frame-src blob:` を追加（PDF の iframe 用）。identifier / productName / version / updater は不変。
- tests: server 依存の 10 ファイル（entities / frontmatter / gemini / path-validation / settings / layout / claude-cli / claude-stream / chat-turn / setup）を削除（Rust 側テストが対応）。

### テスト結果

- `npm ci`・`npm test`（6 ファイル / 83 件）・`npm run typecheck`・`npx nuxt generate`: 通過。
- 追加: `tests/api.test.ts`（全 command の名前と引数、AppError 変換、ブラウザ単体の reject、`chat_send` の Channel 中継 / detach / 入力不正、`read_source_bytes` の Blob 化とキャッシュ）、`tests/chat-events.test.ts`（`applyChatStreamEvent`、一次資料マーカー href）。維持: updater / dialog / slug / update-cask（`isTauriRuntime` の参照元だけ `app/utils/api` に変更）。
- `cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings && cargo test`（90 件）: 通過。

### chat 合流後の確認（4f53963 を merge）

- `chat_send`（`request`+`onEvent`）/ `chat_abort`（`turnId`）の引数名・`ChatRequest`・`ChatStreamEvent`（kebab-case / camelCase）は api.ts と一致。api.ts が呼ぶ全 command が `generate_handler` に登録済み。契約のずれ無し。
- `npm ci && npm test`（83 件）`&& npm run typecheck && npx nuxt generate`、`cargo check`・`clippy --all-targets -D warnings`・`cargo test`（132 件）: 通過。
- `npm run tauri:build`（Linux）: deb / rpm / AppImage の 3 つが成功。deb の内容に `.output/server`・`node_modules`・`.mjs` は無い（`usr/bin/phrapedia` + desktop + icon のみ）。
- xvfb 上で release バイナリを PATH から node / npx を外した環境（PATH=空 dir）で 30 秒起動 → クラッシュ無し（timeout で打ち切り、残存プロセス無し。EGL/DRI の警告のみ）。

### 未検証 / 注意

- 実機 GUI での操作（画像・PDF の表示、`window.open` による Blob URL の「元ファイル」「PDF を開く」「ダウンロード」リンク、WebView 上のアイコン表示、チャットのストリーム表示 / 停止）。いずれも未実施。
- MCP サブコマンド合流後の `tauri:build` 再確認は未実施（integrate-test）。
- `data/amulets/README.md` と `sources/README.md` に旧 API / PGlite の記述が残る（利用者データ側の文書のため未変更）。README / CLAUDE.md / ARCHITECTURE の更新（「デスクトップ専用化、web 版は旧 branch のまま」）は integrate-test。
- `nuxt typecheck` が `vue-router/volar/sfc-route-blocks` の解決警告を出す（型エラーではない。本変更前から出るかは未確認）。

## chat / MCP 節（`claude -p` の Rust 実装と stdio MCP サブコマンド）

### 構成

- `src-tauri/src/chat/`（Tauri 非依存。Channel への配信だけ `commands/chat.rs`）
  - `stream.rs`: stream-json 1 行 → `ChatStreamEvent`（旧 `claude-stream.ts` の移植）。`system/init`（session id）、`stream_event` の text_delta、`assistant` の tool_use（`update_draft` は `draft` に変換し tool-start/result は出さない）、`user` の tool_result（`save_entity` の成功は `saved` も出す）、`result`（エラー）。tool-result の切り詰めは旧実装と同じ 20,000 文字（文字数基準）。
  - `claude.rs`: argv / env / 一時ファイル / プロセス制御。`prompt.rs`・`history.rs`: 旧 `prompt.ts`・`history.ts` の移植。`turn.rs`: 1 ターン（セッション作成、`--session-id` / `--resume`、resume 失敗時の新規セッション + 履歴ダイジェスト、履歴保存、エラー文言の分類）。
  - `commands/chat.rs`: `chat_send`（Channel。完了まで resolve しない）、`chat_abort`（`turnId` → `watch` で中断）。同じ `turnId` の二重実行は `invalid_argument`。WebView が閉じて Channel 送信に失敗したら CLI を中断する。
- `src-tauri/src/mcp/`: `tools.rs`（11 ツール・resource・prompt。rmcp 非依存の純関数で単体テスト可能）、`server.rs`（rmcp の `ServerHandler` アダプタ・引数解析）。`main.rs` は先頭 argv が `mcp` なら Tauri を初期化せずに `mcp::run_cli`。

### 設計判断

- **system prompt を argv に載せない**: `claude --help` の本文に `--system-prompt[-file]` の記載があり、一覧には出ないが `--system-prompt-file <path>` は実機で有効（`PINEAPPLE` とだけ答えさせて確認）。0700 の専用 dir（`<app data dir>/state/claude-work/run-<uuid>/`）に 0600 で書き、実行後（異常終了含む）に `Drop` で削除。強制終了で残った `run-*` は次回起動時に 1 日経過分を掃除。
- **MCP 設定も 0600 ファイル**: `--mcp-config <path>`（`--strict-mcp-config`）。中身は `{"mcpServers":{"phrapedia":{"type":"stdio","command":<current_exe>,"args":["mcp","--config-dir",…,"--data-dir",…]}}}`。ユーザ prompt は stdin。`ps` に system prompt（下書き）も MCP 設定も出ないことを実機で確認（下記）。
- **プロセスグループ**: 子は `process_group(0)` で新しいグループ。中断・timeout は group へ SIGTERM → 5 秒後 SIGKILL。さらに claude 本体が正常終了した後も group へ SIGKILL を送り、孫（`phrapedia mcp` や CLI が起こした子）を残さない。PR #5 レビューの「detached しておらず孫が残りうる」はこれで解消。
- **timeout**: `ai.timeoutMs`（既定 600000）。中断・timeout 時は未完の tool 部分に「（中断されました）」を入れ、中断は assistant message に「（ユーザーが停止しました）」を追加して `done`。
- **cwd**: `<app data dir>/state/claude-work`（専用 dir。データセットではない・親の CLAUDE.md を拾わない）。
- **env**: `CLAUDECODE` `CLAUDE_CODE_ENTRYPOINT` `CLAUDE_CODE_CHILD_SESSION` `CLAUDE_CODE_SESSION_ID` `CLAUDE_CODE_SESSION_ATTENDED` `CLAUDE_CODE_EXECPATH` `CLAUDE_CODE_MESSAGING_SOCKET` `CLAUDE_CODE_MESSAGING_TOKEN` `CLAUDE_PID` を除去（`CLAUDE_CONFIG_DIR` は残す）、`CLAUDE_CODE_MAX_OUTPUT_TOKENS=ai.maxTokens`、PATH は core の `cli::child_path()`、コマンド解決は `cli::resolve_command`。
- **ツール許可**: `--tools ""`、`--allowedTools` は MCP ツール名（`sync_data` 除外）、`--disallowedTools mcp__phrapedia__sync_data`、`--permission-mode dontAsk` + `--permission-prompts none`。
- **claude_session_id**: core の SQLite（`set_claude_session_id`）。CLI が実際に作ったセッション id（`system/init` / `result`）だけ保存。履歴メッセージ削除時のリセットは core 側 command。
- **エラー**: 入力不正（message 空 / slug 不正 / turnId 空・重複）は `invalid_argument` で reject。CLI 未検出・ログイン切れ・モデル不正・利用上限・timeout・異常終了は旧実装と同じ日本語文言の `error` イベント（`chat_send` 自体は resolve）。`prepare` 段階（claude が見つからない）の失敗は `session` より前に `error` だけが流れる（旧 SSE と同じ）。

### crate 選定（MCP）

- **`rmcp` 3.9.0**（`server` / `transport-io`）: Model Context Protocol 公式の Rust SDK（README: "The official Rust SDK for the Model Context Protocol"、modelcontextprotocol/rust-sdk）。保守されていて stdio transport が組み込み。代案の自前 JSON-RPC 実装は initialize の版交渉・通知・ページングを自前で持つことになり、仕様追随の負担が増えるので採らなかった。
- `#[tool]` マクロは使わず `ServerHandler` を手書き: 旧 `server/mcp/**` の説明文・入力スキーマ（日本語を含む）を JSON のまま移植するため。ツール本体は rmcp 非依存の `tools.rs` に置き、rmcp なしで単体テストできる。
- 広告するプロトコル版は `known_up_to(2025-11-25)`（initialize を持つ版まで）。3.x の SDK は新しい 2026-07-28 版も知っているが、現行クライアントは initialize 方式なのでそこで止めた。
- `tokio`（process / io-util / time / rt-multi-thread / macros / sync）: claude の非同期 spawn と MCP の実行基盤。rmcp の feature は `server` + `transport-io` だけ（`macros` / `schemars` は不使用）。

### 確認した claude フラグ（Claude Code 2.1.317、`claude --help` と実機で確認）

`-p` / `--output-format stream-json` / `--verbose` / `--include-partial-messages` / `--model` / **`--system-prompt-file`**（help 本文に記載、実機で有効）/ `--system-prompt-snapshot off` / `--mcp-config <path>` / `--strict-mcp-config` / `--tools ""` / `--allowedTools` / `--disallowedTools` / `--permission-mode dontAsk` / `--permission-prompts none` / `--setting-sources ""` / `--disable-slash-commands` / `--session-id` / `--resume`。`--system-prompt-file` は 2 ターン目の `--resume` でも有効なことを実機確認（下書きを毎回渡し直す設計のまま動く）。

### MCP ツール（旧 `server/mcp/**` からの移植）

| 種別 | 名前 | 備考 |
|---|---|---|
| tool | `list_entities` `get_entity` `entity_template` `save_entity` | `save_entity` は旧と同じ戻り（`ok kind slug path name status committed`）。slug 省略時は `derive_slug`。`git.autoCommit` 有効時のみ commit |
| tool | `list_sources` `read_source` `search_sources` | `read_source` の画像は text + image content。一次資料は読み取り専用 |
| tool | `web_research` | `agy` / `gemini` を core 経由で実行。戻りは `# Web research (UNVERIFIED — …)` + 本文 |
| tool | `sync_status` `sync_data` | チャットでは `sync_data` を許可しない。外部クライアントからは使える |
| tool | `update_draft` | ack（`draft updated (not saved yet)`）のみ。チャット実行器が呼び出しを `draft` に変換 |
| resource | `phrapedia://{kind}/{slug}` | list / templates/list / read |
| prompt | `research_record`（引数 `kind`=monks\|temples, `name`） | NATIVE_API.md の名称に合わせてアンダースコア |

- ツールの失敗は JSON-RPC エラーでなく `isError: true` の結果（メッセージはモデルに見える）。未知のツール名だけ JSON-RPC の invalid params。
- オブジェクトの戻りは整形 JSON のテキスト（旧実装の `JSON.stringify(v, null, 2)` 相当）。

### 外部クライアント（Claude Code 等）から使う

GUI は不要。バイナリのパスと、アプリが使っている設定 dir / データ dir を渡す（アプリの「ホーム → MCP」カードに実値入りの設定例が出る）。省略時は OS 標準の `jp.grrow.phrapedia` 配下を使う。

```jsonc
// Claude Code: claude mcp add-json phrapedia '<下の JSON>'  /  .mcp.json
{
  "mcpServers": {
    "phrapedia": {
      "type": "stdio",
      "command": "/Applications/Phrapedia.app/Contents/MacOS/Phrapedia",   // macOS の例。Linux は AppImage 展開後 / deb の /usr/bin/phrapedia など
      "args": ["mcp", "--config-dir", "<app config dir>", "--data-dir", "<app data dir>"]
    }
  }
}
```

- app config dir / data dir（Tauri の既定）: macOS は `~/Library/Application Support/jp.grrow.phrapedia`（両方同じ）、Linux は `~/.config/jp.grrow.phrapedia` と `~/.local/share/jp.grrow.phrapedia`、Windows は `%APPDATA%\jp.grrow.phrapedia`。
- 起動直後に標準入力が閉じるとプロセスは 0 で終了する。標準出力は MCP 専用（ログは標準エラー）。引数不正は usage を標準エラーに出して 2。
- GUI と別プロセスなので、実行ごとに設定ファイルと SQLite を読み直す（SQLite は WAL + busy_timeout）。

### テスト結果

- `cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings && cargo test`: すべて通過（lib 142 件 + `tests/mcp_stdio.rs` 3 件）。
- stream 変換: system/init・text delta（重複防止）・tool_use/tool_result・画像・切り詰め・`update_draft` → `draft`・`save_entity` → `saved`（失敗時は出さない）・error result・ノイズ/サブエージェントの無視・中断時の未完 tool。ワイヤ形式（kebab-case の type + camelCase）。
- argv: `sync_data` が allowedTools に無く disallowedTools にある・`--tools ""`・session-id / resume の切替・`--mcp-config` と `--system-prompt-file` が「パスだけ」で中身が argv に無い。一時ファイルの 0700/0600 と Drop 削除。
- プロセス制御（ダミー sh スクリプト）: stdin 渡し・env 適用、SIGTERM を無視する子 + 孫が中断で全員死ぬ、timeout で group 全滅、正常終了後に残った孫の回収、既に中断済みなら spawn しない。
- resume 失敗判定、履歴ダイジェスト、エラー文言の分類、リクエスト検証。
- MCP: tools の入出力（temp dataset で list/save/get/search/read、画像、エラー、resource、prompt）。実バイナリの stdio 往復（`tests/mcp_stdio.rs`: initialize → tools/list（名前順・スキーマ）→ tools/call（list_entities / save_entity / get_entity / search_sources / 失敗 / update_draft / 未知ツール）→ resources → prompts、stdin close で exit 0、引数不正で exit 2）。
- **実 claude 2.1.317（204、ログイン済み、`CLAUDECODE` 系を除去）**: `examples/chat_smoke.rs`（`cargo build --bin phrapedia && cargo run --example chat_smoke -- <roundtrip|abort|resume> <work-dir>`）
  - roundtrip: `tool-start`(search_sources) → `tool-result`(count 1) → `draft` → `text-delta` 多数 → 完了、error なし。`claude_session_id` 保存済み。実行中 24 回 `ps` を採取し、system prompt（下書き内の目印文字列を含む）・`mcpServers`・`--system-prompt` が argv に出ないことを確認。終了後に claude / `phrapedia mcp` が残らない。
  - abort: 最初の `tool-start` で中断 → `aborted=true`、parts に「（ユーザーが停止しました）」、子孫（claude・`phrapedia mcp`）が残らない。
  - resume: 2 ターン目が `--resume` で文脈を保持（「1件」と回答）し、セッション id が維持される。

### 未検証 / 注意

- macOS / Windows 実機（GUI 起動 PATH での `claude` 解決、macOS の `Phrapedia` バイナリ名、Windows の `windows_subsystem="windows"` で stdio MCP が使えるか）。Windows は process group の kill が未実装（`killpg` は no-op）で、`--system-prompt-file` / stdio MCP も未確認。
- GUI からの `chat_send`（Tauri 実機 + Channel）。ここでは `run_chat_turn` を直接呼ぶ example で確認した。Linux の Tauri 実機（xvfb）は frontend / integrate-test 側の確認範囲。
- 外部クライアント（Claude Code 以外）との相互接続。Claude Code 2.1.317（`claude -p` の `--mcp-config`）で接続できることだけ実機確認。
- 古い CLI の `--system-prompt-file` / `--system-prompt-snapshot` サポート（最低版は未調査）。
- `web_research` / `sync_data` の実行（MCP 経由の引数検証と単体テストのみ。実 CLI / remote は未実行）。
- `--system-prompt-file` が `--help` の一覧に出ない点は将来の CLI 変更で変わりうる（実機では 2.1.317 で有効）。
- tool-result の切り詰めは JS の UTF-16 長ではなく文字数基準（上限値は同じ 20,000）。

### 追記（レビュー対応）
- `chat_send` の `request` は `serde_json::Value` で受けて手動で解析（`parse_request`）。kind 不正・message 欠落などは Tauri の文字列エラーでなく構造化 `invalid_argument` で reject する。
- resume 時の下書き反映を実機確認: 2 ターン目の下書きを `ALPHA`→`BETA` に変えて聞くと、`--system-prompt-file` + `--resume` でも `BETA`（最新）が system prompt に反映される（モデルが自分の過去の update_draft として `ALPHA` に言及することはある = 会話履歴）。
- frontend の `useChat.ts` は `session` 前の `error` イベントも処理する（grep で確認）。

## integrate 節（結合・実 smoke・文書・PR）

### 既定 branch の取り込み
- `git merge origin/claude/thai-monks-temples-db-6j0hoy`（rebase なし）。PR #5（3aed4c9）・PR #6（91cd7cc）・0.1.3（08a01bc）を取り込み、version は 0.1.3 のまま（package.json / package-lock.json / tauri.conf.json / Cargo.toml / Cargo.lock）。version は上げていない。
- 衝突は rust-native 側（Node 撤去後）を基本に解決。PR #5 の claude CLI 案内、PR #6 の更新アイコン、README「更新方法」は文書に残した。`.mcp.json`（HTTP /mcp 向け）は削除。
- release.yml / tauri.updater.conf.json（pubkey）/ identifier / productName / 成果物名は既定 branch と同一。tauri.conf.json は beforeBuildCommand を `npm run generate` に、CSP から 127.0.0.1 を除去（frame-src blob: を追加）。

### smoke 結果（Linux/204）
- `npm ci` / `npm test`（6 files・83 tests pass）/ `npm run typecheck`（exit 0・TS エラー 0。vue-tsc が `vue-router/volar/sfc-route-blocks` の解決警告を出すが失敗にならない。vue-router 4.6.4 側の exports 由来で、今回の変更とは無関係）。
- `cargo check` / `cargo clippy --all-targets -- -D warnings` / `cargo test`（lib 143 + 3 pass）。
- `npm run tauri:build`（updater overlay 付き）で deb / rpm / AppImage と各 .sig を生成。バンドルはバイナリのみで `.output/server` は無い。
- xvfb で PATH から node / npx を外した環境（PATH は gh スタブのみ）で 30 秒起動してクラッシュなし（timeout で終了、exit 124）。
- `phrapedia mcp --config-dir --data-dir` に stdio で initialize → tools/list（11 tool）→ tools/call(list_entities) を流し、同梱 data/ のコピーで monks 2 件を確認。
- 同梱 data/ の全レコード（monks 2・temples 2）を MCP の save_entity で保存し直し、差分は updated_at のみ。
- 実 claude（~/.local/bin/claude）でチャット 1 往復（examples、CLAUDECODE 系 env を除去）: tool-start / tool-result / draft / text-delta / 完了を確認。実行中の ps に system prompt・MCP 設定の中身は出ず、終了後に子・孫プロセスなし。
- git 同期（一時 repo／bare remote）、sources 検索、設定 2 パスの検証（存在しない／書けないパス、データ dir がリポジトリ外）は Rust テスト（core::git / core::path_settings / core::sources）で確認。
- `scripts/release.sh --dry-run 0.1.4`（bare repo + gh スタブ）が完走。release.yml は js-yaml で parse し、build / cask 両 job の run 部を確認（HOMEBREW_TAP_TOKEN / HOMEBREW_TAP_SSH_KEY の分岐、updater 署名の有無の両分岐を維持）。Cask は `depends_on formula "node"` なし、tests/update-cask.test.ts pass。

### 非互換
- 旧 PGlite データ（チャット履歴・FTS・調査キャッシュ）は移行しない。レコードと一次資料は Markdown なのでそのまま使える。
- HTTP `/mcp` 廃止（`phrapedia mcp` stdio へ）。
- デスクトップ専用。ブラウザ版・Render 配備は旧 branch のまま。`start` / `start:render` / `build` / `preview` / render.yaml / scripts/render-start.mjs は残置（この構成では動かない。削除は未判断）。

### 未検証
- macOS / Windows 実機（署名・notarize・dmg・updater の実更新・GUI 起動 PATH での claude 解決）。
- GUI からの chat_send（Tauri 実機 + Channel）。chat 往復は core の example で確認。
- 外部クライアント（Claude Code 以外）との MCP 接続。web_research（agy）・sync_data（実 remote）の実行。
- GitHub Actions 上での release workflow 実行（ローカルでは YAML parse と run 部確認、release.sh は dry-run まで）。
- src-tauri 内に旧 Nitro 実装を指すコメント（tools.rs / commands/mod.rs / git.rs / store.rs / path_settings.rs の "sidecar"）が残る。履歴説明なので触っていない。
