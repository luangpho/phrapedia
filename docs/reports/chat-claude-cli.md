# チャット AI の実行方式を Claude Code CLI（`claude -p`）に置き換え

## 目的
- アプリ内チャットを Anthropic SDK（API キー必須）から、ユーザの `claude` ログイン（Pro / Max の契約枠）で動く Claude Code CLI の headless 実行に置き換える。API キー設定を不要にする。
- 変えないもの: チャット AI は Claude 固定 / Gemini は `web_research` のみ / SSE 契約（`ChatStreamEvent`）/ フロント（`useChat.ts`）。

## 設計
| 項目 | 内容 |
|---|---|
| 実行 | `server/utils/agent/claude-cli.ts` が `claude -p` を spawn。プロンプトは stdin、stdout の stream-json を 1 行ずつ `claude-stream.ts` の `ClaudeStreamParser` で `ChatStreamEvent` に変換 |
| MCP | `--mcp-config` で自身の `/mcp`（`http://127.0.0.1:<受信ソケットのポート>/mcp`、サーバ名 `phrapedia`）。Basic 認証有効時は `headers.Authorization` を付与 |
| ツール許可 | `--tools ""`（組み込み全無効）+ `--allowedTools` に MCP ツール名を列挙（`sync_data` 除外）+ `--disallowedTools mcp__phrapedia__sync_data` |
| update_draft | MCP ツール `server/mcp/tools/update-draft.ts`（戻りは ack のみ、説明文に「アプリ内チャット専用」）。`tool_use` を見て `draft` イベントに変換（`tool-start/result` は出さない） |
| セッション | `chat_sessions.claude_session_id`（migration 2）。初回 `--session-id <uuid v4>`、以降 `--resume <id>`。resume 失敗（`No conversation found`）は新規セッション + PGlite 履歴のテキスト要約を prompt 先頭に添えて再実行。履歴メッセージ削除時は ID をリセット |
| 中断 / timeout | `AbortSignal` → SIGTERM、5 秒後 SIGKILL。`ai.timeoutMs`（既定 600000）で同様に終了。中断時は parts に「（ユーザーが停止しました）」 |
| 子プロセス env | `process.env` を継承し、`CLAUDECODE` / `CLAUDE_CODE_ENTRYPOINT` / `CLAUDE_CODE_CHILD_SESSION` / `CLAUDE_CODE_SESSION_ID` / `CLAUDE_CODE_SESSION_ATTENDED` / `CLAUDE_CODE_EXECPATH` / `CLAUDE_CODE_MESSAGING_SOCKET` / `CLAUDE_CODE_MESSAGING_TOKEN` / `CLAUDE_PID` を除去（`CLAUDE_CONFIG_DIR` は残す: ログイン情報の場所）。`CLAUDE_CODE_MAX_OUTPUT_TOKENS` に `ai.maxTokens`。PATH に `/opt/homebrew/bin` `/usr/local/bin` `~/.local/bin` `~/.claude/local` を補う |
| cwd | `os.tmpdir()/phrapedia-claude-work`（空の専用 dir。データセットに書き込ませない・親ディレクトリの CLAUDE.md を拾わない） |
| 設定 | `ai { model, maxTokens, extraInstructions, command, args, timeoutMs }`。`capabilities.claudeFound / claudeVersion`（`claude --version`、60 秒キャッシュ）。旧 `ai.anthropicModel` は読み込み時に `ai.model` へ移行 |
| env | `NUXT_AI_MODEL`（旧 `NUXT_ANTHROPIC_MODEL`）, `NUXT_CLAUDE_COMMAND`。`NUXT_ANTHROPIC_API_KEY` は廃止 |
| Tauri | sidecar に渡す PATH に `/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin` を（未包含のものだけ）追加（`sidecar_path()`）。それまでは親プロセスの PATH をそのまま継承していた（GUI 起動は最小 PATH。`find_node` が PATH 外も探すだけで sidecar には渡していなかった）。cask は変更なし |

## 確認したフラグ（Claude Code 2.1.317、`claude --help` で実在を確認）
`-p` / `--output-format stream-json` / `--verbose` / `--include-partial-messages` / `--model` / `--system-prompt` / `--system-prompt-snapshot off` / `--mcp-config` / `--strict-mcp-config` / `--tools ""` / `--allowedTools` / `--disallowedTools` / `--permission-mode dontAsk` / `--permission-prompts none` / `--setting-sources ""` / `--disable-slash-commands` / `--session-id` / `--resume`（`--version`）。
- `--system-prompt-snapshot off` が必須: 既定（on）だと resume 時に初回の system prompt が固定され、最新の下書きが反映されない（ALPHA → BETA で実測。off で BETA になる）。
- `--safe-mode` は不採用: `--mcp-config` まで無効になる（MCP 接続 0 件を実測）。代わりに `--setting-sources ""` + `--disable-slash-commands` で、ユーザ個人の hooks / plugin / skills（実測でユーザの出力スタイル hook がチャットに混入した）を除去。
- `--bare` は不採用: OAuth / keychain を読まず API キー専用になる（help の記述）。

## テスト結果
- `npm test`: 12 files / 117 tests passed（新規: `claude-stream`, `claude-cli`, `chat-turn`, `settings`）
- `npm run typecheck`: pass / `npm run build`: pass / `npm run smoke`: 8/8 pass（`/mcp` の tools は 11 件。`update_draft` を含む）
- `cargo check` / `cargo clippy -- -D warnings` / `cargo test`: pass（16 tests、新規 `sidecar_path_appends_missing_dirs_only`）
- 実 smoke（204、claude 2.1.317 ログイン済み、`CLAUDECODE` 等を除去、空きポート、Basic 認証あり/なし）:
  - 実応答の SSE（session → tool-start / tool-result → draft → text-delta → done）
  - 2 ターン目の `--resume` で文脈保持、各ターンの最新下書きが system prompt に反映
  - セッションファイルを削除して再送 → 新規セッションで履歴要約が効き、error は出ない
  - 中断（curl 切断）→ 「（ユーザーが停止しました）」が保存され、`claude` 子プロセスが残らない
  - `command` を存在しないパスにすると 400 相当の日本語 `error` イベント、`capabilities.claudeFound=false`
  - Basic 認証有効時も `/mcp` が通る（ツール呼び出し成功）
  - `sync_data` はチャットから呼ばれない

## 残課題 / 未検証
- Render: `render.yaml` に `NUXT_ANTHROPIC_API_KEY` が残っている（アプリは読まない）。Render でチャットを使うには build で `claude` を導入し、CLI が読む `ANTHROPIC_API_KEY` を環境に置く必要がある。未対応・未検証（スコープ外のため触っていない）。
- macOS / Tauri 実機（GUI 起動の PATH で `claude` を解決）は未検証。Rust 側の PATH 組み立ては単体テストのみ。
- Windows は未検証。
- `--system-prompt-snapshot` は 2.1.317 で確認。古い CLI では未知フラグでエラーになる可能性（最低版は未調査）。
- ユーザ個人の `~/.claude/CLAUDE.md` は `--setting-sources ""` でも読まれるかを断定していない（出力スタイル hook は除去を確認）。
- `server/utils/mcp-bridge.ts` の `createMcpBridge()`（インプロセス MCP）はチャットからは使わなくなった（未使用。`listMcpToolNames()` だけ使用）。削除はしていない。
- 長い下書き（最大 60,000 文字）は `--system-prompt` の argv に載る。Linux の単一引数上限（約 128 KB）以内。
