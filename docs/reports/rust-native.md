# Rust + Tauri ネイティブ化

branch: `feat/rust-native`。実行時の Node / Nitro / PGlite を無くし、WebView の SPA + Rust の Tauri command だけで動かす。
この文書は worker ごとの節に分かれる。**core 節**は core worker が書いた（Rust core と command 層）。chat / frontend / integrate-test / review の各節は担当 worker が追記する。

契約（command 名・引数・戻り値・エラー型・`ChatStreamEvent`・MCP 起動形）は [docs/NATIVE_API.md](../NATIVE_API.md)。

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

### 未検証 / 注意

- 実機 GUI での操作（画像・PDF の表示、`window.open` による Blob URL の「元ファイル」「PDF を開く」「ダウンロード」リンク、WebView 上のアイコン表示、チャットのストリーム表示 / 停止）。いずれも未実施。
- `chat_send` / `chat_abort` は chat worker の実装待ち（frontend は契約どおり呼ぶのみ）。
- `data/amulets/README.md` と `sources/README.md` に旧 API / PGlite の記述が残る（利用者データ側の文書のため未変更）。README / CLAUDE.md / ARCHITECTURE の更新（「デスクトップ専用化、web 版は旧 branch のまま」）は integrate-test。
- `nuxt typecheck` が `vue-router/volar/sfc-route-blocks` の解決警告を出す（型エラーではない。本変更前から出るかは未確認）。
