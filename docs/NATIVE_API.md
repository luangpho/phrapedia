# Phrapedia ネイティブ API 契約（Rust + Tauri v2）

Node / Nitro / PGlite を実行時に使わない新構成の、WebView（SPA）↔ Rust の契約。
frontend worker は `$fetch('/api/...')` をこの契約の `invoke(...)` に置換し、chat worker は chat command と MCP サブコマンドを実装する。
**この文書が正**。変更が要る時は core worker に ask し、文書と実装を同時に直す。

## 0. 前提と全体像

| 層 | 実体 |
|---|---|
| WebView | Nuxt SPA（build 時のみ node）。`@tauri-apps/api/core` の `invoke` / `Channel` だけでバックエンドと通信。HTTP サーバは無い |
| Rust core | `src-tauri/src/core/**`（Tauri 非依存。GUI の command と MCP サブコマンドの両方から使う） |
| Tauri command 層 | `src-tauri/src/commands/**`（`#[tauri::command]`。core を薄く包む） |
| 状態 DB | SQLite（`<app data dir>/state/phrapedia.sqlite`）。チャット / 検索 index / Web 調査キャッシュ。**旧 PGlite データは移行しない** |
| 設定 | app config dir: `settings.json`（2 パス。従来互換）と `app-settings.json`（AppSettings。ai / gemini / git ほか） |

- 引数名は **camelCase**（Tauri が Rust 側の snake_case に変換する）。戻り値のフィールドも camelCase（既存 TS 型 `shared/types/**` と同じ。レコードの frontmatter キーだけは YAML そのままの snake_case）。
- 既存の型（`EntityKind` `EntityRecord` `EntitySummary` `SourceFile` `SourceSearchHit` `SourceContent` `SyncStatus` `SyncResult` `ChatMessage` `ChatPart` `ChatSession` `AppSettings` `SettingsView`）は **`shared/types/**` の定義を維持**する（フィールド名・意味は変更しない。ただし `SettingsView.resolved.projectRoot` は廃止し `datasetRoot` / `exportRoot` に置換 = §4）。
- 長い処理（git / web_research / reindex）は Rust 側で `spawn_blocking` 済み。JS 側は普通に `await invoke` すればよい。

## 1. エラー型

すべての command は失敗時に次の JSON を **reject** する（`invoke` の catch で受ける。文字列ではない）。

```ts
interface AppError {
  /** 機械判定用。下表 */
  code: AppErrorCode
  /** 日本語。そのまま UI に出してよい */
  message: string
  /** 任意。原因の補足（stderr 末尾、OS のエラー等）。UI は折りたたみで出す程度 */
  detail?: string
}
type AppErrorCode =
  | 'invalid_argument'   // kind / slug / パラメータ不正（REST の 400）
  | 'not_found'          // 404
  | 'conflict'           // 作成時に既存など（409）
  | 'path_unusable'      // データセット / 書き出し先パスが使えない（旧 409）
  | 'invalid_frontmatter'// YAML frontmatter が壊れている
  | 'io'                 // その他の入出力エラー
  | 'git'                // git 実行の失敗（SyncResult で返す操作の「実行不能」系）
  | 'cli_not_found'      // claude / agy / gemini が起動できない
  | 'cli_failed'         // CLI が異常終了 / 出力が空
  | 'timeout'            // CLI がタイムアウト
  | 'aborted'            // chat_abort / 呼び出し側中断
  | 'internal'
```

TS 側ヘルパ（frontend が用意）: `isAppError(e): e is AppError`、`errorMessage(e) = isAppError(e) ? e.message : String(e)`。

- 「Git 管理外」「コミットする変更が無い」「push 失敗」など **想定内の結果は reject せず `SyncResult { ok:false }`** で返す（旧挙動と同じ）。
- `sync_status` は旧挙動どおり reject しない（`isRepo:false` + `error`）。

## 2. command 一覧（REST との対応）

引数はオブジェクト `invoke('cmd', { ... })`。戻り値は `Promise<T>`。

### 2.1 アプリ / パス設定

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `app_info` | なし | `AppInfo` | `sidecar_info` + `GET /api/health` |
| `get_path_settings` | なし | `PathSettingsView` | 同名（維持） |
| `save_path_settings` | `{ datasetPath: string, exportPath: string }` | `SavePathsOutcome` | 同名。**サーバ再起動は無い**（保存 = 即反映） |

```ts
interface PathCheckView { ok: boolean; error: string | null }
interface AppInfo {
  version: string                  // アプリ版（Cargo package version）
  settingsWarning: string | null   // settings.json / app-settings.json が壊れていた等
  datasetPath: string              // 実効パス
  exportPath: string
  datasetCheck: PathCheckView
  exportCheck: PathCheckView
  stateDir: string                 // SQLite の置き場
}
interface PathSettingsView {       // 従来互換
  datasetPath: string; exportPath: string; effectiveExport: string
  datasetCheck: PathCheckView; exportCheck: PathCheckView; settingsFile: string
}
interface SavePathsOutcome {       // restartError は廃止
  ok: boolean; datasetError: string | null; exportError: string | null
  settings: PathSettingsView
}
```

- 不正パスでもアプリは起動する。レコード / 資料 / git を使う command は `path_unusable`（message: 「データセットパスが使えません: …」/「書き出し先パスが使えません: …」）で reject。設定系・チャット履歴系は動く。
- パスは **絶対パス必須**。未設定時の既定データセットは `<app data dir>/dataset`（初回に自動作成）。

### 2.2 設定（AppSettings）

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `get_settings` | なし | `SettingsView` | `GET /api/settings`（CLI の `--version` 検出込み。claude の結果は 60 秒キャッシュ） |
| `update_settings` | `{ patch: DeepPartial<AppSettings> }` | `AppSettings` | `PUT /api/settings`（検証は旧 zod と同じ範囲。違反は `invalid_argument` + 日本語 message） |
| `reindex_sources` | なし | `{ indexed, removed, total }` | `POST /api/sources/reindex`（useSettings.reindex） |

`AppSettings` は `shared/types/settings.ts` のとおり（`sourcesDir` `dataDir` `ai` `gemini` `git`。`DEFAULT_SETTINGS` も同値を Rust 側に持つ）。`update_settings` はユーザの上書き分だけ保存する（旧挙動）。旧 `ai.anthropicModel` は読み込み時に `ai.model` へ移行。

```ts
// SettingsView = AppSettings + 派生情報（projectRoot を datasetRoot/exportRoot に置換）
interface SettingsView extends AppSettings {
  resolved: { sourcesDir: string; dataDir: string; datasetRoot: string; exportRoot: string }
  capabilities: { claudeFound: boolean; claudeVersion?: string; geminiFound: boolean; geminiVersion?: string; geminiEngine: string }
}
```

### 2.3 レコード（entities）

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `list_entities` | `{ kind }` | `EntitySummary[]` | `GET /api/entities/:kind` |
| `entity_template` | `{ kind, name?: string }` | `string`（raw markdown） | `GET …/template`（**`{raw}` ではなく文字列そのもの**） |
| `create_entity` | `{ kind, raw, message?: string }` | `SaveEntityResult` | `POST /api/entities/:kind`（slug は frontmatter から導出） |
| `get_entity` | `{ kind, slug }` | `EntityRecord` | `GET …/:slug`（無ければ `not_found`） |
| `update_entity` | `{ kind, slug, raw, message?: string }` | `SaveEntityResult` | `PUT …/:slug` |
| `delete_entity` | `{ kind, slug }` | `{ ok: true, commit: SyncResult \| null }` | `DELETE …/:slug` |

```ts
interface SaveEntityResult { record: EntityRecord; commit: SyncResult | null }
```

- `commit` は `git.autoCommit` 有効時のみ非 null（旧 `autoCommitIfEnabled`）。失敗しても保存自体は成功扱い。
- `EntityRecord.path` は **データセット root からの相対 POSIX パス**（例 `data/monks/x.md`。旧 `projectRoot` 相対と実質同じ）。
- 書込は書き出し先、読出は書き出し先優先 → データセット fallback（ARCHITECTURE.md の「読み書きと git 同期の規則」を維持）。

### 2.4 一次資料（sources）

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `list_sources` | `{ dir?: string }` | `SourceFile[]` | `GET /api/sources` |
| `search_sources` | `{ q: string, limit?: number }` | `SourceSearchHit[]` | `GET /api/sources/search`（limit 1..50、既定 20。空クエリは `[]`） |
| `read_source` | `{ path: string, maxChars?: number }` | `SourceContent` | `GET /api/sources/file/<path>`（JSON 版） |
| `read_source_bytes` | `{ path: string }` | `ArrayBuffer`（`tauri::ipc::Response`） | `?raw=1`（画像 / PDF の表示用） |

- 一次資料は **読み取り専用**。書込 command は存在しない。パスは `..`・絶対パス・symlink 脱出を `invalid_argument` で拒否。
- 画像の表示: `read_source_bytes` → `new Blob([buf], { type: mime })` → `URL.createObjectURL`。mime は `SourceFile.ext` から JS 側で決める（png/jpg/jpeg/gif/webp/pdf）。asset protocol は使わない（パスが動的で scope が不安定なため）。CSP の `img-src` に `blob:` が要る（既存のまま）。
- 検索は部分一致（大文字小文字無視、タイ語 / 日本語対応）。スコア規則は旧実装と同じ。

### 2.5 Web 調査

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `web_research` | `{ query, context?, images?: string[], ocrText?: string, noCache?: boolean }` | `ResearchResult` | `POST /api/research` |

```ts
interface ResearchResult { text: string; engine?: string; model?: string; cached: boolean; durationMs: number; stderr?: string }
```

プロンプト構築（`buildResearchPrompt`）・画像の作業 dir へのコピー・キャッシュ（`gemini.cacheHours`、キー = model + 画像ハッシュ + prompt の SHA-256）・JSON 封筒の unwrap は Rust 側。失敗は `cli_not_found` / `cli_failed` / `timeout`。

### 2.6 Git 同期

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `sync_status` | なし | `SyncStatus` | `GET /api/sync/status` |
| `commit_data` | `{ message?: string }` | `SyncResult` | `POST /api/sync/commit` |
| `pull_data` | なし | `SyncResult` | `POST /api/sync/pull` |
| `push_data` | なし | `SyncResult` | `POST /api/sync/push` |
| `sync_data` | `{ message?: string }` | `SyncResult` | `POST /api/sync/sync` |

型は `shared/types/sync.ts` のまま。ステージ対象は書き出し先の data ディレクトリのみ。

### 2.7 チャット履歴（store）

| command | 引数 | 戻り値 | 置換元 |
|---|---|---|---|
| `list_chat_sessions` | `{ kind, slug?: string \| null }` | `ChatSession[]` | `GET /api/chat/sessions` |
| `get_chat_session` | `{ id }` | `{ session: ChatSession, messages: ChatMessage[] }` | `GET …/:id` |
| `delete_chat_session` | `{ id }` | `{ ok: true }` | `DELETE …/:id` |
| `patch_chat_session` | `{ id, patch: { slug?: string \| null, title?: string } }` | `ChatSession` | `PATCH …/:id` |
| `delete_chat_messages` | `{ id, ids: string[] }` | `{ ok: true, deleted: number }` | `DELETE …/:id/messages`（削除が 1 件以上なら `claudeSessionId` をリセット） |

## 3. チャット（chat worker 担当・契約のみ）

REST の SSE `POST /api/chat` を **Channel 配信の command** に置換する。

```ts
import { Channel, invoke } from '@tauri-apps/api/core'

interface ChatRequest {          // shared/types/chat.ts の ChatRequest に turnId を追加
  turnId: string                 // クライアントが採番する UUID。chat_abort のキー（初回はまだ sessionId が無いため）
  sessionId?: string
  kind: EntityKind
  slug?: string | null
  message: string
  draft: string
}

const ch = new Channel<ChatStreamEvent>()
ch.onmessage = (ev) => { /* 旧 SSE の data: と同じ ChatStreamEvent */ }
await invoke<void>('chat_send', { request, onEvent: ch })   // ターン完了（done / error 送出後）で resolve
await invoke<void>('chat_abort', { turnId })                // 進行中ターンを中断（SIGTERM → 5 秒後 SIGKILL）。無ければ no-op
```

- `chat_send` は **完了まで resolve しない**（ストリームは Channel）。最初のイベントは必ず `session`、最後は `done` か `error`。入力不正（message 空 / kind 不正）は reject（`invalid_argument`）。それ以外のターン内エラーは `{ type: 'error', message }` イベントで流し、`chat_send` 自体は resolve する（旧 SSE と同じ）。
- 中断時の振る舞い（parts に「（ユーザーが停止しました）」を残して `done`）は旧実装のまま。
- `ChatStreamEvent` は `shared/types/chat.ts` のものを **変更しない**:

```ts
type ChatStreamEvent =
  | { type: 'session'; sessionId: string; userMessageId: string; assistantMessageId: string }
  | { type: 'text-delta'; delta: string }
  | { type: 'tool-start'; toolCallId: string; name: string; input: unknown }
  | { type: 'tool-result'; toolCallId: string; name: string; output: unknown; isError?: boolean }
  | { type: 'draft'; markdown: string; summary?: string }
  | { type: 'saved'; kind: EntityKind; slug: string; path?: string }
  | { type: 'done'; message: ChatMessage }
  | { type: 'error'; message: string }
```

Rust では `#[serde(tag = "type", rename_all = "kebab-case")]` の enum（フィールドは camelCase）。

### 3.1 chat worker が使う core API（Rust）

`phrapedia_lib::core::Core`（`Arc` で共有、スレッドセーフ。`Core::new(AppDirs { config_dir, data_dir })`）:

- `core.settings()` / `core.update_settings(patch)` / `core.settings_view()`
- `core.store()`: `create_chat_session` `get_chat_session` `list_chat_sessions` `append_chat_message`（同 id は parts を上書き）`list_chat_messages` `delete_chat_session` `delete_chat_messages` `touch_chat_session` `set_claude_session_id`（旧 `chat-store.ts` と 1 対 1）
- `core.entities()`: `list` `get` `exists` `derive_slug` `write` `delete` `template`
- `core.sources()`: `list` `read` `search` `reindex` `absolute_path`
- `core.git()`: `status` `commit` `pull` `push` `sync` `auto_commit_if_enabled`
- `core.research()`: `run` `probe`、`build_research_prompt`
- `core::cli::resolve_command(command) -> PathBuf`、`core::cli::child_env()`（GUI 起動の最小 PATH を補った env。`/opt/homebrew/bin` `/usr/local/bin` `$HOME/.local/bin` `$HOME/.claude/local` と login shell の PATH）

## 4. MCP サブコマンド（chat worker 担当・契約のみ）

claude CLI に渡す MCP は **自分自身のバイナリの stdio サブコマンド**（HTTP /mcp は廃止）。

```jsonc
// claude の --mcp-config（--strict-mcp-config）に渡す
{ "mcpServers": { "phrapedia": {
  "type": "stdio",
  "command": "<std::env::current_exe()>",
  "args": ["mcp", "--config-dir", "<app config dir>", "--data-dir", "<app data dir>"]
} } }
```

- `phrapedia mcp ...` は **GUI を起動せず**（`main.rs` で最初に argv を見て分岐。Tauri を初期化しない）、stdio で MCP を話す。AppDirs は引数で受ける（Tauri 無しで config dir を解決しないため。省略時は OS 標準 + identifier `jp.grrow.phrapedia`）。
- サーバ名は `phrapedia`、ツール名は従来どおり（`mcp__phrapedia__<tool>`）。ツールは `list_entities` `get_entity` `save_entity` `entity_template` `list_sources` `search_sources` `read_source`（画像は image content）`web_research` `sync_status` `sync_data` `update_draft`（ack のみ）。入出力スキーマと説明文は `server/mcp/tools/*.ts` を移植元とする（チャットでは `sync_data` を許可しない規則も維持）。リソース `phrapedia://{kind}/{slug}` とプロンプト `research_record` も同様。
- 実行ごとに設定ファイル・SQLite を読み直す（GUI プロセスと別プロセスなので、書込は SQLite の WAL + busy_timeout で共存）。

## 5. frontend 向け: 置換対応表

| 旧（`$fetch` / SSE） | 新（`invoke`） |
|---|---|
| `sidecar_info` + `setApiBase(baseUrl)` | `app_info`（`phase` / `baseUrl` は無い。常に即応答） |
| `restart_sidecar` | **廃止**（再起動不要。設定画面の「再起動」UI は削除） |
| `GET /api/health` | `app_info` |
| `GET /api/settings` | `get_settings` |
| `PUT /api/settings` | `update_settings({ patch })` |
| `GET /api/entities/:kind` | `list_entities({ kind })` |
| `GET /api/entities/:kind/template?name=` | `entity_template({ kind, name })` → `string` |
| `POST /api/entities/:kind` | `create_entity({ kind, raw, message })` |
| `GET /api/entities/:kind/:slug` | `get_entity({ kind, slug })` |
| `PUT /api/entities/:kind/:slug` | `update_entity({ kind, slug, raw, message })` |
| `DELETE /api/entities/:kind/:slug` | `delete_entity({ kind, slug })` |
| `GET /api/sources?dir=` | `list_sources({ dir })` |
| `GET /api/sources/search?q=&limit=` | `search_sources({ q, limit })` |
| `GET /api/sources/file/<p>`（JSON） | `read_source({ path, maxChars })` |
| `GET /api/sources/file/<p>?raw=1` | `read_source_bytes({ path })` → Blob URL |
| `POST /api/sources/reindex` | `reindex_sources` |
| `POST /api/research` | `web_research({ … })` |
| `GET /api/sync/status` | `sync_status` |
| `POST /api/sync/{commit,pull,push,sync}` | `commit_data` / `pull_data` / `push_data` / `sync_data` |
| `GET /api/chat/sessions?kind=&slug=` | `list_chat_sessions({ kind, slug })` |
| `GET/DELETE /api/chat/sessions/:id` | `get_chat_session` / `delete_chat_session` |
| `PATCH /api/chat/sessions/:id` | `patch_chat_session({ id, patch })` |
| `DELETE /api/chat/sessions/:id/messages` | `delete_chat_messages({ id, ids })` |
| `POST /api/chat`（SSE） | `chat_send({ request, onEvent: Channel })` + `chat_abort({ turnId })` |
| `get_path_settings` / `save_path_settings` | 同名（維持。`restartError` 廃止） |
| `POST /mcp`（HTTP） | `phrapedia mcp` stdio サブコマンド（§4） |

HTTP 層の `cors` / `csrf` / `auth` middleware は不要（サーバが無い）。CSP の `connect-src` から `http://127.0.0.1:*` を外し、`img-src` は `'self' data: blob:` にできる（tauri.conf.json の CSP は frontend/integrate-test の担当。core は capabilities のみ最小化する）。

## 6. 非互換・移行（要約。詳細は docs/reports/rust-native.md）

- 旧 PGlite（`state/pglite`）のチャット履歴・設定・Web 調査キャッシュは **移行しない**。AppSettings は既定値から開始（2 パスの `settings.json` は維持）。
- 検索 index・Web 調査キャッシュは SQLite に作り直し（再 index は自動）。
- `NUXT_*` / `PHRAPEDIA_ROOT` などの環境変数による設定上書きは廃止（デスクトップ専用化）。
- Render デプロイ / `npm run dev` の Web 版は新構成では提供しない（server/ 削除は frontend worker）。
