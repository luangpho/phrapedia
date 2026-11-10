# Phrapedia アーキテクチャ

タイの高僧・寺院・プラクルアン（お守り）の情報を、**Markdown ファイルを正本**として蓄積するローカルファースト型の知識ベース。デスクトップ専用（Rust + Tauri v2）。実行時に Node / Nitro / PGlite は使わない。

> 旧構成（Nuxt SSR + Nitro の REST API / HTTP `/mcp` / PGlite）は旧 branch のまま。Web 版（Render）は 0.2.0 で終了。履歴は tag `web-legacy-render`。この文書は新構成（`feat/rust-native` 由来）の説明。command 単位の契約は [NATIVE_API.md](NATIVE_API.md)。

## 技術スタック

| 層 | 採用技術 |
|---|---|
| バックエンド | Rust（Tauri v2 の command）。中核 `src-tauri/src/core/` は Tauri 非依存で、GUI と `phrapedia mcp` が共有 |
| フロント | Nuxt 4 を **SPA 静的生成専用**（`ssr:false`、`nuxt generate` → `.output-frontend`）、TypeScript strict。WebView が読み、Tauri `invoke` / `Channel` でバックエンドを呼ぶ |
| UI | Nuxt UI v4（Dashboard / Chat コンポーネント）, Tailwind CSS v4, `@tailwindcss/typography` |
| アプリ状態 DB | SQLite（`rusqlite` bundled、WAL + busy_timeout）— チャット履歴・一次資料の FTS5 インデックス（trigram）・Web 調査キャッシュ。`<app data dir>/state/phrapedia.sqlite` |
| 設定 | app config dir の JSON 2 ファイル: `settings.json`（データセット / 書き出し先パス）と `app-settings.json`（ai / gemini / git ほか。上書き分だけ保存） |
| MCP | `rmcp` による stdio サーバ。同じバイナリのサブコマンド `phrapedia mcp`。アプリ内チャットも外部クライアントも同じ定義を使う |
| 一次情報 | `sources/`（データセットパス配下）の Markdown / 画像。**最優先・確定情報**として扱う。読み取り専用 |
| Web 調査 | Google の CLI を子プロセスで呼び出す。既定は **Antigravity CLI**（`agy`）、旧 **Gemini CLI**（`gemini`、プロンプトは stdin）も選択可。結果は常に「未確認」扱い |
| チャット AI | Claude 固定。Claude Code CLI を子プロセス `claude -p`（stream-json）で実行し、ユーザの `claude` ログインで動く（API キー不要）。Gemini は `web_research` 経由のインターネット検索にのみ関与 |
| 同期 | git CLI を spawn。`data/` をコミット → `pull --rebase` → `push`（GitHub） |

node / npm は **ビルド時のみ**（`npm run generate` と tauri CLI）。配布物（`.dmg` / deb / rpm / AppImage）に `.output/server` や Node は入らない。

## ディレクトリ

```
app/                      Nuxt SPA（pages, components, composables, layouts, utils）
  utils/api.ts                   バックエンド窓口（invoke ラッパ、chatSend、sourceBlobUrl）
  components/chat/Panel.vue      <ChatPanel> AI チャット窓
  components/markdown/*.vue      Markdown 表示・編集
  components/entity/*.vue        サイドバー一覧など
  components/desktop/*.vue       デスクトップ層（パス設定、更新アイコン、バナー）
  composables/useEntities.ts / useChat.ts / useSettings.ts / useSync.ts / useUpdater.ts ...
shared/types/             app 共通の型（EntityKind, EntityRecord, AppSettings, ChatStreamEvent, SyncStatus ...）
shared/utils/slug.ts      slug 生成
src-tauri/src/
  main.rs                 先頭引数 `mcp` なら stdio MCP、それ以外は GUI
  lib.rs                  plugin 登録（dialog / process / 条件付き updater）、Core を state に載せる、command 登録
  core/                   paths, path_settings, app_settings, frontmatter, entities, slug, sources, store(SQLite),
                          git, research(agy/gemini), cli(外部 CLI の解決と実行), error, types
  commands/               #[tauri::command] 層。chat.rs が chat_send / chat_abort
  chat/                   claude(argv/env/プロセス制御), stream(stream-json→ChatStreamEvent), prompt, history, turn
  mcp/                    tools(11 ツール・resource・prompt), server(rmcp アダプタ)
src-tauri/examples/chat_smoke.rs  実 claude でのチャット手動確認
src-tauri/capabilities/   WebView の権限（core:default / dialog:allow-open / dialog:allow-message / updater:default / process:allow-restart）
data/<kind>/<slug>.md     レコード（正本, git 管理）  kind = monks | temples | amulets(将来)
sources/                  一次資料（サンプル。実運用はデータセットパス配下）
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

`core/entities.rs` が slug / kind / name / created_at / updated_at を正規化して書き戻す。未知のキーは保持される。frontmatter の出力は旧実装（npm の `yaml`、`lineWidth: 0`）と同じ整形を自前で再現しており、既存レコードを開いて保存しても差分は `updated_at` の 1 行だけ。

## Tauri command

REST は無い。フロントは `app/utils/api.ts` 経由で `invoke('<command>', {...})` を呼ぶ。一覧と引数・戻り値・エラー型は [NATIVE_API.md](NATIVE_API.md)。分類:

| 分類 | command |
|---|---|
| アプリ / パス | `app_info` `get_path_settings` `save_path_settings` `updater_enabled` |
| 設定 | `get_settings` `update_settings` `reindex_sources` |
| レコード | `list_entities` `entity_template` `get_entity` `create_entity` `update_entity` `delete_entity` |
| 一次資料 | `list_sources` `search_sources` `read_source` `read_source_bytes` |
| Web 調査 | `web_research` |
| Git | `sync_status` `commit_data` `pull_data` `push_data` `sync_data` |
| チャット | `list_chat_sessions` `get_chat_session` `delete_chat_session` `patch_chat_session` `delete_chat_messages` `chat_send` `chat_abort` |

エラーは日本語 message 付きの構造化 `AppError`（`code`: `not_found` / `invalid_argument` / `conflict` / `path_unusable` ほか）。api.ts が `ApiError` に変換する。ブラウザ単体（Tauri 無し）では各 call が `desktop_only` で reject する。

## チャット AI の設計

1. フロントは `chat_send` に `{ turnId, sessionId?, kind, slug, message, draft }` と Tauri `Channel` を渡す。`draft` はエディタの現在の Markdown 全文。`turnId` はクライアント採番 UUID で、中断（`chat_abort`）の指定に使う。`chat_send` は完了まで resolve しない。
2. Rust が `claude -p` を子プロセスとして起動する（`chat/claude.rs`）。ユーザ発話は stdin、system prompt（`chat/prompt.rs`）は 0600 の一時ファイルを `--system-prompt-file`、ツールは `--mcp-config <0600 ファイル>` で渡した **自バイナリの `phrapedia mcp`**（stdio）。一時ファイルは 0700 の専用 dir（`<app data dir>/state/claude-work/run-<uuid>/`）に置き、実行後に削除する。**`ps` に system prompt も MCP 設定の中身も出ない。**
   組み込みツールは `--tools ""` で無効、`--allowedTools` は MCP ツール名の列挙（`sync_data` を含めない。`--disallowedTools` でも deny）、`--permission-mode dontAsk --permission-prompts none` で承認待ちを作らない。ユーザ個人の Claude Code 設定が混ざらないよう `--setting-sources ""` `--disable-slash-commands`。作業ディレクトリは上記の専用 dir。
   チャット専用ツール `update_draft({ markdown, summary })` は MCP ツール（戻りは ack のみ）。実行器が stream-json の `tool_use`（`mcp__phrapedia__update_draft`）を見て `{ type: 'draft', markdown }` を流し、エディタが更新される（ファイルは保存しない。保存はユーザーの操作）。
3. システムプロンプトの原則: 一次資料（search_sources / read_source）を最優先 → 不足分のみ web_research（Gemini）→ Web 由来は「未確認」と明記 → 出典セクションに一次資料パスと URL を分けて書く → 高僧の追加では高僧中心、寺院の追加では寺院中心。
4. ストリーミングは `Channel` で `ChatStreamEvent`（`shared/types/chat.ts`）を送る。CLI の stdout（`--output-format stream-json --verbose --include-partial-messages`）を 1 行ずつ `chat/stream.rs` で変換する:
   `stream_event` の text_delta → `text-delta` / `assistant` の `tool_use` → `tool-start`（update_draft は `draft`）/ `user` の `tool_result` → `tool-result`（20,000 文字で切り詰め。save_entity 成功時は `saved`）/ `result` の失敗 → `error`。
   中断（`chat_abort`）・WebView が閉じて Channel 送信に失敗・timeout（`ai.timeoutMs`）では、子の process group へ SIGTERM（5 秒後 SIGKILL）。claude 本体が正常終了した後も group へ SIGKILL を送り、孫（`phrapedia mcp` など）を残さない（Unix。Windows は未実装）。子プロセスの env から `CLAUDECODE` 等「Claude Code 内」を示す変数は除去し、`CLAUDE_CODE_MAX_OUTPUT_TOKENS` に `ai.maxTokens` を渡す。
5. セッション継続: SQLite の `chat_sessions.claude_session_id` に CLI のセッション ID を保持し、初回は `--session-id <uuid v4>`、2 回目以降は `--resume <id>`。システムプロンプトにエディタの最新下書きを含めるため `--system-prompt-snapshot off`。resume に失敗した時（別マシン・セッションファイル消失）は新しいセッションで再実行し、SQLite の履歴をテキストの要約（`chat/history.rs`）としてプロンプト先頭に添える。会話のメッセージ削除（リトライ）時は ID をリセットする。
   セッションは (kind, slug) に紐づき、slug が null の下書きセッションはブラウザの localStorage で記憶され、保存後に `patch_chat_session` で slug へ付け替えられる。
6. 役割分担: Claude が会話・一次資料の読解・下書きの執筆を担い、Web からの情報収集だけを `web_research`（Gemini）に委ねる。Gemini の出力は検索結果の要約＋出典 URL で、レコード本文は Claude が書く。
7. 画像を対象にした指示（「この画像の〜」）: Claude は `read_source` で画像を確認して文字を読み取り、`web_research` に `images`（一次資料の画像パス）と `ocrText`（自分の読み取り結果＝参考情報）を渡す。
   `core/research.rs` は画像を実行ごとの作業ディレクトリ（`<app data dir>/state/gemini-work/run-*`）にコピーし、その中で CLI を起動する（Gemini CLI は `@image-1.jpg` 構文、Antigravity CLI はファイル読み取りツール）。プロンプトは「画像本体を必ず自分で開く／文字を自分で読み取る／完全一致が確証できなければ拡大・部分切り出しで読み直す／候補と確信度を併記／原語の文字列で検索」を必須手順として指示する。キャッシュキーには画像内容のハッシュを含める。

## MCP（stdio）

`phrapedia mcp --config-dir <app config dir> --data-dir <app data dir>`（省略時は OS 標準の `jp.grrow.phrapedia` 配下）。GUI を起動せず stdio で MCP を話す。標準出力は MCP 専用（ログは標準エラー）、標準入力が閉じると 0 で終了、引数不正は usage を出して 2。GUI と別プロセスなので、実行ごとに設定ファイルと SQLite を読み直す。

- tool: `list_entities` `get_entity` `entity_template` `save_entity` `list_sources` `read_source` `search_sources` `web_research` `sync_status` `sync_data` `update_draft`
- resource: `phrapedia://{kind}/{slug}`、prompt: `research_record`
- ツールの失敗は `isError: true` の結果（メッセージはモデルに見える）。

外部クライアント設定例は README の「MCP で使う」。HTTP `/mcp` は廃止した。

## デスクトップ層と 2 つのパス設定

| 設定 | 意味 | 既定 |
|---|---|---|
| データセットパス | 読み出し元の root。直下に `data/<kind>/`（`monks` `temples` `amulets` の Markdown）と `sources/` を持つ | `<app data dir>/dataset`（初回に自動作成） |
| 書き出し先パス | 書き込み先の root（同じ配置の `data/<kind>/`）。git 操作の対象 | データセットパスと同じ |

- 保存先: app config dir の `settings.json`。保存 = 即反映（サーバ再起動は無い）。フォルダ選択は `tauri-plugin-dialog`。パスは絶対パス必須。
- 検証: 「絶対パスでない」「存在しない」「ディレクトリでない」「書き込めない（書き出し先）」を検出し、UI（設定ページとエラーバナー）に表示する。**不正パスでもアプリは落とさず起動**し、レコード / 資料 / git の command は `path_unusable` で reject する（設定系・チャット履歴系は動く）。

### 読み書きと git 同期の規則

1. 書き込み（保存・削除・リネーム）は常に書き出し先。データセット側のファイルは変更・移動・削除しない（読み取り専用でも使える）。
2. 読み出しは書き出し先を優先し、同じ slug が書き出し先に無ければデータセットへ fallback する。一覧はデータセット → 書き出し先の順にマージし、同 slug は書き出し先が勝つ。`sources/` はデータセット側のみ。
3. **git 操作（commit / pull / push）の対象は書き出し先**。書き出し先が git リポジトリの時だけ有効。リポジトリでなければ同期 UI は「Git 管理外」と表示し、commit / pull / push は実行しない。ステージするのは書き出し先配下の data ディレクトリのみ。
4. 書き出し先 = データセットパスの時は読み書きとも同一 root。

## 外部 CLI の解決

`claude` / `agy` / `gemini` / `git` は、継承 PATH → login shell の PATH（`$SHELL -lc`、4 秒 timeout、プロセス内で 1 回）→ 固定候補（`/opt/homebrew/bin` `/usr/local/bin` `$HOME/.local/bin` など）の順で探す（GUI 起動は shell の PATH を引き継がないため）。設定の command が `/` を含めばそのまま使う。子プロセスは新しい process group で起動し、timeout 時は group ごと SIGTERM → 2 秒後 SIGKILL。

## 配布

macOS（aarch64 / x64）は GitHub Actions（tauri-action）で `.dmg` を作り、Homebrew cask（`luangpho/homebrew-tap`）で配布する。cask は Node に依存しない。Linux は `tauri:build` で deb / rpm / AppImage が出る（CI の配布対象外）。アプリ内更新は署名鍵が揃ったビルドだけで有効（README「更新方法」）。
