# Phrapedia

タイの **高僧・寺院・プラクルアン（お守り）** の情報を、AI と一緒に調べて **Markdown ファイル** として蓄積するローカルファースト型の知識ベースです。**デスクトップアプリ専用**（Rust + Tauri v2。実行時に Node は不要）です。

- 正本は `data/<種類>/<slug>.md`（YAML frontmatter + Markdown）。Git で履歴管理し、GitHub に同期できます。
- 手元の **一次資料**（`sources/` のメモ・画像）を最優先し、足りない部分だけを **Google の CLI（Antigravity CLI）による Web 調査** で補います。Web 由来の情報は常に「未確認」として扱います。
- 同じツール群を **stdio の MCP サーバ**（`phrapedia mcp`）として公開しているので、Claude Code や Antigravity CLI からも読み書きできます。

> 内部構成の詳細は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)、フロントとバックエンドの契約は [docs/NATIVE_API.md](docs/NATIVE_API.md) を参照してください。

### 旧版（Web 版）からの変更点

この構成は旧 Nuxt SSR + Node 版の全面再実装です。次の点が旧版と非互換です。

- **デスクトップ専用**になりました。Web 版（Render）は 0.2.0 で終了。履歴は tag `web-legacy-render`。
- **旧 PGlite（`.phrapedia/`）のデータは移行されません**（チャット履歴・一次資料の検索インデックス・Web 調査キャッシュ・**旧版で保存したアプリ設定（AI / Gemini / Git の設定）**）。設定は既定値に戻るので、必要なら設定画面で入れ直してください（`settings.json` の 2 パスは同じ場所・同じキーなのでそのまま使えます）。インデックスとキャッシュは自動で作り直されます。チャット履歴は引き継がれません。レコード（`data/`）と一次資料は Markdown なのでそのまま使えます。
- **HTTP の `/mcp`（`http://localhost:3000/mcp`）は廃止**しました。代わりに `phrapedia mcp`（stdio）を使います（[MCP で使う](#mcp-で使う)）。
- REST API・SSE は無く、フロントは Tauri command を直接呼びます。
- **対応環境は macOS（Homebrew cask / dmg）と Linux（deb / rpm / AppImage）**です。Windows は未検証で、子プロセスの process group 単位の終了（`claude` の孫プロセスの回収）が未実装（no-op）です。
- `.env` は読みません。パスは設定画面、`GEMINI_API_KEY` などはプロセスの環境変数で渡します。

---

## 目次

1. [機能](#機能)
2. [必要なもの](#必要なもの)
3. [クイックスタート](#クイックスタート)
4. [Homebrew でインストール](#homebrew-でインストール)
5. [情報の優先順位](#情報の優先順位)
6. [画面の流れ](#画面の流れ)
7. [設定リファレンス](#設定リファレンス)
8. [Git 同期](#git-同期)
9. [MCP で使う](#mcp-で使う)
10. [開発コマンド](#開発コマンド)
11. [リリース手順](#リリース手順)
12. [tap リポジトリの作り方](#tap-リポジトリの作り方)
13. [署名・notarization の設定](#署名notarization-の設定)
14. [トラブルシューティング](#トラブルシューティング)
15. [ロードマップ](#ロードマップ)

---

## 機能

| 機能 | 内容 |
|---|---|
| レコード管理 | 高僧・寺院のレコードを一覧・閲覧・編集・削除。Markdown プレビュー付きエディタ |
| AI チャット | レコードごとのチャット窓。AI が一次資料を検索し、Web 調査で補い、**下書き** をエディタに反映（保存はユーザーが行う） |
| 一次資料 | 任意フォルダの Markdown / 画像を全文検索（タイ語・日本語・英語の部分一致）・プレビュー |
| Web 調査 | Antigravity CLI `agy`（Google 検索）をアプリから実行。旧 Gemini CLI も選択可。結果はキャッシュ |
| Git 同期 | `data/` だけをコミット → `pull --rebase` → `push`。保存ごとの自動コミット／自動プッシュも可 |
| MCP サーバ | `phrapedia mcp`（stdio）。外部の AI エージェントから同じ操作が可能 |
| ダークモード | Nuxt UI のカラーモードに対応 |

## 必要なもの

実行に必要なもの（Node.js は **不要**。`node` / `npm` はソースからビルドする時だけ必要です）:

- **Claude Code CLI（`claude`）**（チャット AI に使用。別途インストールが必要で、**API キーは不要**）
  ```bash
  claude --version     # 入っているか確認（導入手順: https://docs.claude.com/en/docs/claude-code/overview）
  claude               # 一度対話起動して、Claude のアカウント（Pro / Max など）でログインしておく
  ```
  - チャットは `claude -p`（headless 実行）を子プロセスとして呼び出し、**`claude` にログインしたアカウントの契約枠**で動きます。`ANTHROPIC_API_KEY` 等が環境にあれば CLI がそちらを使いますが、アプリ側では何も扱いません。
  - `claude` が見つからない場合は 設定 →「AI」の「コマンド」に絶対パスを指定します。
  - チャット AI は **Claude 固定** です。Gemini（`agy`）はチャットの代わりにはならず、**インターネット検索による情報収集（`web_research` ツール）にだけ** 使われます。
- **Git**（同期を使う場合。GitHub の認証は通常の `git push` が通る状態にしておく）
- **Antigravity CLI（`agy`）**（任意。Web 調査に使用。2026 年 6 月に無料枠 / Google One 向けの Gemini CLI を置き換えた Google 公式 CLI。無くてもチャットと一次資料検索は使えます）
  ```bash
  curl -fsSL https://antigravity.google/cli/install.sh | bash   # ~/.local/bin/agy に入る
  agy --version
  ```
  認証はどちらか一方:
  - このマシンで一度 `agy` を対話起動して **Google アカウントでサインイン** しておく（資格情報は OS のキーリングに保存され再利用されます）
  - 環境変数 `GEMINI_API_KEY`（[Google AI Studio](https://aistudio.google.com/) で発行）を設定し、`~/.gemini/antigravity-cli/settings.json` に `{"modelProvider": "gemini"}` を書く
  - 旧 **Gemini CLI**（`npm i -g @google/gemini-cli`、有料枠 / API キー利用者向け）を使い続ける場合は 設定 →「CLI」で `Gemini CLI` を選ぶ

ソースからビルドする場合のみ: Node.js 22 以上、Rust ツールチェーン、Tauri の OS 別前提パッケージ（[Tauri Prerequisites](https://v2.tauri.app/start/prerequisites/)）。

## クイックスタート

利用者は [Homebrew](#homebrew-でインストール)（macOS）か、GitHub Release のインストーラを使います。ソースから動かす場合:

```bash
git clone <このリポジトリ> phrapedia && cd phrapedia
npm ci
npm run tauri:dev     # 開発起動（nuxt dev --port 1420 + tauri dev）
npm run tauri:build   # インストーラ生成（Linux: deb / rpm / AppImage、macOS: dmg。出力は src-tauri/target/release/bundle/）
```

`npm run dev` を単体のブラウザで開いても Tauri command が無いため動きません（各画面に「この機能はデスクトップアプリ専用です」と出ます）。

初回起動時に、アプリデータ dir（macOS: `~/Library/Application Support/jp.grrow.phrapedia`、Linux: `~/.local/share/jp.grrow.phrapedia`）に `state/phrapedia.sqlite`（チャット履歴・検索インデックス・Web 調査キャッシュ）と既定のデータセット `dataset/` が作られます。レコードを置く場所は 設定画面の **データセットパス** で変えられます（このリポジトリの clone をそのまま指定すると、同梱のサンプルが見えます）。

サンプルとして以下のレコードと一次資料が同梱されています（いずれも `status: draft`、「要確認」箇所あり）。

| 種類 | レコード |
|---|---|
| 高僧 | `data/monks/somdet-to.md`（ソムデット・トー）, `data/monks/luang-pu-thuat.md`（ルアンプー・トゥアット） |
| 寺院 | `data/temples/wat-rakhang.md`（ワット・ラカン）, `data/temples/wat-chang-hai.md`（ワット・チャーンハイ） |
| 一次資料 | `sources/notes/somdet-to.md`, `sources/notes/wat-rakhang.md`（サンプル） |

### セキュリティ上の注意

Phrapedia はログイン機構のない「自分の PC で動かすツール」です。

- 配布版は待ち受けるネットワークポートを持ちません（HTTP サーバは無し）。外部クライアントとの接続は `phrapedia mcp` の stdio だけです。
- 設定画面の値（CLI のコマンドパス、フォルダなど）はそのまま使われます。
- 認証情報（`GEMINI_API_KEY` など）はアプリに保存しません。環境変数で渡し、子プロセス（`agy` / `gemini`）に引き継がれます。Claude のログイン情報は `claude` CLI が保持します。
- チャットの `claude -p` に渡す system prompt と MCP 設定は 0600 の一時ファイルで渡し、`ps` には中身が出ません。
- アプリ内のチャット AI には Git 同期ツール（`sync_data`）を渡していません。GitHub への push は常に同期パネルからの操作です（自動プッシュ設定を除く）。

## Homebrew でインストール

macOS（Apple Silicon / Intel）向けのデスクトップアプリ（Tauri）を Homebrew cask で配布します。

```bash
brew tap luangpho/tap
brew install --cask phrapedia
```

- cask は Node に依存しません（アプリは単体の Rust バイナリです）。
- 初回起動後、設定画面で **データセットパス**（レコード・一次資料の読み出し元）と **書き出し先パス**（保存先・Git 同期の対象）を指定します（詳細は [設定リファレンス](#設定リファレンス)）。
- インストール後そのまま起動できます。cask がインストール時に隔離属性（quarantine）を外します（ad-hoc 署名のビルド）。
- 更新: `brew upgrade --cask phrapedia`、またはアプリのサイドバー上部の更新アイコン（下記「更新方法」）。削除: `brew uninstall --cask phrapedia`（`--zap` でアプリの設定・キャッシュも削除）。
- チャットには **Claude Code CLI（`claude`）を別途インストール**してください（cask は `claude` に依存しません）。アプリは PATH に加えて login shell の PATH と `/opt/homebrew/bin`・`/usr/local/bin`・`$HOME/.local/bin` などを探して `claude` / `agy` / `git` を見つけます。それ以外の場所にある場合は 設定 →「AI」でコマンドの絶対パスを指定してください。
- 注意: Finder / Launchpad から起動した GUI アプリはシェルの環境変数を引き継ぎません。`GEMINI_API_KEY` などは、ターミナルから `open -a Phrapedia` で起動するか、`launchctl setenv` で渡してください。

## 情報の優先順位

| 優先度 | 情報源 | 扱い |
|---|---|---|
| 1 | **一次資料**（`sources/` 配下、設定で変更可） | 確定情報。AI は必ず最初に検索・参照する |
| 2 | **Web 調査**（Antigravity CLI / Gemini CLI） | **未確認**。一次資料に無い部分だけを補う。本文で「未確認」と明記 |

役割分担: **Claude**（チャット AI）が会話・一次資料の読解・下書きの執筆を行い、インターネット検索による情報収集だけを **Gemini**（`web_research` ツール）に委ねます。

**画像についての指示**（「この画像の○○について調べて」など）の場合:

1. Claude が一次資料の画像本体を開いて自分で文字を読み取る。
2. Web 調査には質問文だけでなく **画像本体**（`images`）と **Claude が読み取った文字**（`ocrText`、参考情報）を必ず渡す。
3. Gemini 側は画像を自分で開いて文字を読み取り直し、参考テキストや検索結果と**完全一致が確証できない場合は画像の拡大・部分ごとの読み直し**を行い、候補と確信度を併記したうえで、読み取った原語の文字列で検索する。
4. 両者の読み取りが食い違う箇所はレコードに「要確認」として残す。

レコードの `## 出典` には次のように分けて書きます。

```markdown
## 出典
- 一次資料: notes/somdet-to.md
- Web（未確認）: https://example.com/...
```

`status` は `draft`（下書き）→ `reviewed`（確認済み）→ `verified`（検証済み）の順に人が上げていきます。

## 画面の流れ

```
┌ サイドバー ──────────┐ ┌ メイン ─────────────────────────┐ ┌ チャット ───────┐
│ [高僧] [寺院] タブ    │ │ レコード表示 / Markdown エディタ │ │ AI と対話        │
│  検索・一覧           │ │                                  │ │ → 一次資料検索   │
│  ＋ 高僧を追加        │ │  [編集] [AIに相談] [保存]        │ │ → Web 調査       │
│  ───────────────      │ │                                  │ │ → 下書きを反映   │
│  Git 同期 / 一次資料  │ │                                  │ │                 │
│  設定                 │ └──────────────────────────────────┘ └─────────────────┘
└──────────────────────┘
```

1. サイドバーの **タブ**（高僧 / 寺院）で種類を選び、一覧から開く。
2. **高僧を追加 / 寺院を追加** ボタン（＋）で新規作成画面へ。テンプレートが入ったエディタの右に **チャットパネル** が開く。既存レコードでは **AIに相談** で同じパネルを開ける。
3. チャットに「ソムデット・トーについて調べて」などと頼むと、AI が
   一次資料検索 → 必要なら Web 調査 → **下書き**（エディタの内容）を更新 します。この時点ではファイルは保存されません。
4. 内容を確認・手直しして **保存**。`data/<種類>/<slug>.md` に書き込まれます（slug は frontmatter の `slug` / `name_en` / `name` から自動生成、重複時は `-2` などを付与）。
5. サイドバー下部の **Git 同期** ボタン（未コミット件数を表示）で GitHub と同期（または設定で自動コミット）。

そのほか、**一次資料** 画面で資料の一覧・検索・プレビュー、**設定** 画面でフォルダや AI の設定ができます。

## 設定リファレンス

設定画面の値は app config dir の `app-settings.json`（上書きした分だけ保存、既定値は読み込み時に補完）に保存されます。項目は `shared/types/settings.ts` の `AppSettings` に対応します。

| 項目 | キー | 既定値 | 説明 |
|---|---|---|---|
| 一次資料フォルダ | `sourcesDir` | `sources` | データセットパスからの相対パス（または絶対パス） |
| データフォルダ | `dataDir` | `data` | `data/<種類>/<slug>.md` の置き場所（データセットパスからの相対位置）。Git 同期するならリポジトリ内に |
| Claude モデル | `ai.model` | `claude-sonnet-5-5` | `claude --model` に渡す（旧設定 `ai.anthropicModel` は読み込み時に移行） |
| 最大トークン | `ai.maxTokens` | `8192` | 1 回の応答の最大出力トークン（`CLAUDE_CODE_MAX_OUTPUT_TOKENS` で渡す） |
| 追加の指示 | `ai.extraInstructions` | （空） | システムプロンプト末尾に追加される指示 |
| Claude コマンド | `ai.command` | `claude` | 実行ファイル名または絶対パス |
| Claude 追加引数 | `ai.args` | （空） | `claude -p` に追加で渡す引数（例: `--effort low`） |
| Claude タイムアウト | `ai.timeoutMs` | `600000` | 1 回の応答がこれを超えたら CLI を終了させる |
| CLI | `gemini.engine` | `antigravity` | `antigravity`（`agy`）または `gemini-cli`（旧 `gemini`） |
| コマンド | `gemini.command` | `agy` | 実行ファイル名または絶対パス（例: `~/.local/bin/agy`） |
| 追加引数 | `gemini.args` | `[]` | CLI に渡す追加引数（1 行に 1 つ。例: `--print-timeout`, `--effort`） |
| モデル | `gemini.model` | （空 = CLI 既定） | agy は `--model`、gemini は `-m` に渡す（例: `flash`, `pro`） |
| 承認モード | （固定） | `auto` | agy は `--mode auto`、gemini は `--approval-mode auto_edit` を常に付与。変更したい場合は追加引数で `--mode ...` / `--approval-mode ...` を指定すると上書きされる |
| タイムアウト | `gemini.timeoutMs` | `180000` | CLI 1 回あたりの上限（ミリ秒）。超過時はプロセスを終了 |
| キャッシュ時間 | `gemini.cacheHours` | `24` | 同じ調査結果を再利用する時間。`0` で無効 |
| リモート | `git.remote` | `origin` | push / pull 先のリモート名 |
| ブランチ | `git.branch` | （空 = 現在のブランチ） | 同期するブランチ |
| 作者名 / メール | `git.authorName` / `git.authorEmail` | （空 = git の設定） | コミット作者（リポジトリローカル設定に書き込み） |
| 自動コミット | `git.autoCommit` | `false` | レコードの保存・削除ごとにコミット |
| 自動プッシュ | `git.autoPush` | `false` | 自動コミット後に push |

### 保存場所とパス設定

次の 2 つのパスは設定画面で指定します（app config dir の `settings.json` に保存。保存すると即反映され、再起動は不要です）。

| 項目 | 意味 | 既定値 |
|---|---|---|
| データセットパス | 読み出し元。直下に `data/<kind>/`（monks, temples, amulets）と `sources/` を持つ root。リポジトリの clone をそのまま指定できる | アプリデータ dir 配下 `dataset` |
| 書き出し先パス | 書き込み先。同じ配置の root。Git 同期の対象（git リポジトリの時のみ） | データセットパスと同じ |

絶対パスでない・存在しない・ディレクトリでない・書き込めない（書き出し先）パスはエラーバナーに表示されます（アプリは落ちません）。その間、レコード・一次資料・Git の操作は「データセットパスが使えません: …」「書き出し先パスが使えません: …」で失敗し、設定とチャット履歴は使えます。

| 場所 | 内容 |
|---|---|
| app config dir（macOS: `~/Library/Application Support/jp.grrow.phrapedia`、Linux: `~/.config/jp.grrow.phrapedia`） | `settings.json`（2 パス）、`app-settings.json`（上記の項目） |
| app data dir（macOS は同上、Linux: `~/.local/share/jp.grrow.phrapedia`） | `state/phrapedia.sqlite`（チャット履歴・検索インデックス・調査キャッシュ。消しても再生成され、失われるのは履歴とキャッシュだけ）、`state/claude-work/`・`state/gemini-work/`（CLI の作業 dir）、`dataset/`（既定のデータセット） |

Web 調査用の `GEMINI_API_KEY` は画面からは設定できません。アプリを起動するプロセスの環境変数として渡してください（`.env` は読みません）。チャットは `claude` のログイン（または環境の `ANTHROPIC_API_KEY`）を CLI がそのまま使うので、アプリ側の設定はありません。

## Git 同期

データセット（`data/`）はこのリポジトリの一部です。同期は次の順で行います。

1. **コミット** — データフォルダ **だけ** を `git add` してコミット（アプリのコードや `sources/` はステージしない）。
2. **pull --rebase** — 設定したリモート／ブランチから取り込み。
3. **push**

- サイドバー下部の Git 同期ボタンで状態（未コミットの変更数、ahead / behind、最終コミット）を確認し、個別に コミット / Pull / Push / 同期 を実行できます。
- 設定で **自動コミット** を有効にすると保存・削除のたびにコミット、さらに **自動プッシュ** を有効にするとその都度 push します。
- コンフリクトが起きた場合はアプリは自動解決しません。ターミナルで通常の Git 操作で解決してください。
- データフォルダをリポジトリ外に設定した場合、同期機能は使えません。
- 一次資料（`sources/`）を共有したい場合は、自分で Git 管理するか別の手段（クラウドドライブ等）を使ってください。

## MCP で使う

HTTP の `/mcp` は廃止しました。外部クライアントは同じバイナリのサブコマンド **`phrapedia mcp`**（stdio）を起動して接続します。GUI は不要で、標準入力が閉じると終了します。

```
<バイナリ> mcp --config-dir <app config dir> --data-dir <app data dir>
```

- バイナリ: macOS は `/Applications/Phrapedia.app/Contents/MacOS/Phrapedia`、Linux は `/usr/bin/phrapedia`（deb / rpm）や展開した AppImage 内のバイナリ。
- `--config-dir` / `--data-dir` は **アプリが使っているものと同じ値** を渡します（データセットパスなどの設定を読むため）。アプリのホーム画面の「MCP」カードに、実際の値を入れた設定例が出ます。省略するとその OS 標準の `jp.grrow.phrapedia` 配下を使います。
- GUI とは別プロセスなので、呼び出しごとに設定ファイルと SQLite（WAL）を読み直します。

### Claude Code

```bash
claude mcp add-json phrapedia '{"type":"stdio","command":"/Applications/Phrapedia.app/Contents/MacOS/Phrapedia","args":["mcp","--config-dir","<app config dir>","--data-dir","<app data dir>"]}'
```

または `.mcp.json`:

```json
{
  "mcpServers": {
    "phrapedia": {
      "type": "stdio",
      "command": "/Applications/Phrapedia.app/Contents/MacOS/Phrapedia",
      "args": ["mcp", "--config-dir", "<app config dir>", "--data-dir", "<app data dir>"]
    }
  }
}
```

### Antigravity CLI / Gemini CLI（旧）

どちらも stdio の MCP サーバ定義（`command` と `args`）を追加します。上記と同じ値を使ってください。設定場所は `agy mcp add` / `~/.gemini/config/mcp_config.json`（Antigravity）、`~/.gemini/settings.json` の `mcpServers`（Gemini CLI）。**これらのクライアントでの接続は未確認です**（確認済みは Claude Code のみ）。

### ツール一覧

| ツール | 説明 |
|---|---|
| `list_entities` | 保存済みレコード（monks / temples / amulets）の slug・名前・状態の一覧（`kind` 必須） |
| `get_entity` | 1 件のレコードの Markdown 全文（frontmatter 含む） |
| `save_entity` | レコードの作成・更新（Markdown 全文 `markdown` を渡す。slug 自動生成、自動コミット設定に従う） |
| `entity_template` | 新規レコード用のテンプレート（frontmatter + 見出し） |
| `list_sources` | 一次資料フォルダのファイル一覧 |
| `search_sources` | 一次資料の全文検索（部分一致、タイ語・日本語・英語） |
| `read_source` | 一次資料を読む（テキストは本文、画像は画像として返す） |
| `web_research` | Antigravity CLI / Gemini CLI で Web 調査（結果は **未確認** 扱い、キャッシュあり）。`images` に一次資料の画像パス、`ocrText` に読み取り済みの文字を渡すと、画像本体を CLI に添付して読み取り直し＋検索させる |
| `sync_status` | データフォルダの Git 状態 |
| `sync_data` | データフォルダをコミット → pull --rebase → push |
| `update_draft` | アプリ内チャット専用の下書き更新（外部クライアントから呼んでも ack を返すだけ） |

このほか、リソース `phrapedia://{kind}/{slug}`（レコード本文）と、プロンプト `research_record`（一次資料 → Web 調査 → 保存 の手順ガイド）を提供します。ツールの失敗は `isError: true` の結果で返ります。

## 開発コマンド

```bash
npm ci
npm run dev         # nuxt dev --port 1420（Tauri の devUrl。単体のブラウザでは Tauri command が無く機能しない）
npm run generate    # SPA を .output-frontend に静的生成
npm run typecheck   # nuxt typecheck
npm test            # vitest（tests/**/*.test.ts）
npm run tauri:dev   # デスクトップアプリを開発起動（Rust ツールチェーンが必要）
npm run tauri:build # デスクトップアプリをビルド（src-tauri/target/release/bundle/ に出力）
cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings && cargo test   # Rust 側
```

`tauri:build` は `src-tauri/tauri.bundle.conf.json` を重ねてビルドします（`beforeBuildCommand` が `npm run generate`）。同梱するのは Rust バイナリと静的な画面だけで、Node は同梱しません。macOS 向けの配布物は GitHub Actions（下記）で作ります。

実 `claude` でのチャット確認（手動。`claude` にログイン済みの環境で、`CLAUDECODE` 系の環境変数を外して実行）:

```bash
cd src-tauri && cargo build --bin phrapedia
cargo run --example chat_smoke -- roundtrip /path/to/work   # 往復（tool 呼び出し・draft・done、ps に prompt/MCP 設定が出ないこと）
cargo run --example chat_smoke -- abort /path/to/work       # 中断で子・孫プロセスが残らないこと
cargo run --example chat_smoke -- resume /path/to/work      # --resume で文脈と最新下書きが反映されること
```

Web 版（Render）は 0.2.0 で終了。履歴は tag `web-legacy-render`。

## リリース手順

利用者の macOS 端末で **1 コマンド**です（`git` / `node` / `npm` / `gh`（`gh auth login` 済み）が必要）。

```bash
scripts/release.sh 0.1.1            # 版は x.y.z（v は付けない）
scripts/release.sh --dry-run 0.1.1  # push・tag 以外を全部行う（確認用）
```

スクリプトは次を順に行います。手作業の版上げ・タグ打ちは不要です。

1. 引数と重複の検査（版の形式、同名の release / tag、`gh auth status`）。
2. 既定 branch を一時 dir に新規 clone し、`npm ci` / `npm test` / `npm run typecheck`。
3. `package.json` / `package-lock.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml` / `src-tauri/Cargo.lock` の version を揃えて 1 commit（`phrapedia <version>`）。各ファイルの変更行数が想定どおりかを確認してから commit する。
4. 既定 branch と tag `v<version>` を push し、release workflow の完了を待つ。成功したら `gh release view` と tap の `Casks/phrapedia.rb` の version を表示し、一時 dir を消す。

ビルドは GitHub Actions（`.github/workflows/release.yml`）が行い、ローカルでは `tauri build` しません。CI の node は `npm ci` / `npm run generate`（フロントの静的生成）と cask 更新スクリプトにだけ使い、配布物には含まれません。

- macOS（aarch64 / x86_64）をビルドし、GitHub Release に `Phrapedia_<version>_{aarch64,x64}.dmg` を添付する。
- 続く `cask` ジョブが dmg の sha256 を計算して `packaging/homebrew/update-cask.mjs` で cask に書き込み、`luangpho/homebrew-tap` の `Casks/phrapedia.rb` へ push する。リポジトリ secret `HOMEBREW_TAP_SSH_KEY`（推奨）か `HOMEBREW_TAP_TOKEN` のどちらかが必要で、両方無ければこのジョブはスキップされる（両方あれば SSH 鍵を優先）。`HOMEBREW_TAP_TOKEN` は設定済みで、fine-grained token への差し替えは不要。
- アプリ内更新の鍵が登録済みなら、さらに `Phrapedia_<version>_{aarch64,x64}.app.tar.gz` / `.sig` と `latest.json`（`darwin-aarch64` / `darwin-x86_64` の両方入り）が添付される（下記）。

### 更新方法

- 利用者: `brew upgrade --cask phrapedia`。
- アプリ内更新: サイドバー上部の更新アイコンから行う（アイコンはデスクトップ版で、更新用の署名鍵つきでビルドされた版にだけ出る。ブラウザ単体では出ない。サイドバーを折りたたむとフッターに移る）。起動時と 6 時間ごとに黙って更新を確認し、あればアイコンにバッジが付き、tooltip に新しい版が出る（自動ダイアログは出ない）。アイコンを押すと、版と release notes を確認ダイアログに出し、了承すると download（進捗 % を表示）→ インストール → 再起動する。更新が無い状態で押すと、その場で確認して「最新です（現在 x.y.z）」と表示する。現在の版は通常時の tooltip に出る。失敗は通知で表示される。更新の確認・配布には署名鍵が使われる（下記）。
- 権限不足で更新できない場合（Homebrew を別のユーザーで入れ、`/Applications/Phrapedia.app` の所有者が今のユーザーと違う時）は、Homebrew を入れたユーザーで `brew upgrade --cask phrapedia` を実行する。

### アプリ内更新（tauri updater）の鍵

鍵は生成済みで、秘密鍵はリポジトリ secret `TAURI_SIGNING_PRIVATE_KEY` に登録済みです。パスワードは無し（workflow は `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` を空で渡す）。公開鍵は `src-tauri/tauri.updater.conf.json` の `plugins.updater.pubkey` に commit 済みです。

workflow は **secret が空でなく、かつ pubkey が空でない時だけ** `--config src-tauri/tauri.updater.conf.json` を重ねて updater 用の成果物（`.app.tar.gz` / `.sig` / `latest.json`）を作ります。どちらかが空なら従来どおり dmg だけです。有効時は tauri-action が同名の `.app.tar.gz` を `.sig` 付きで上げるため、鍵なし時だけ使う「Attach .app.tar.gz」step はスキップされます。

build job は `strategy.max-parallel: 1` で aarch64 → x64 の順に直列で走ります。`latest.json` は各 job が既存を download → merge → upload するため、並列だと片方の platform が消える競合があり得るからです（所要時間は増える）。

鍵を作り直す場合は `npx tauri signer generate -w ~/.tauri/phrapedia-updater.key` で作り、公開鍵を pubkey に commit し、秘密鍵を secret に登録し直します（秘密鍵は commit しない）。作り直すと、既に配布済みのアプリは新しい署名の更新を検証できなくなります。

### cask を手動で更新する場合

```bash
node packaging/homebrew/update-cask.mjs --version 0.1.0 \
  --arm-dmg Phrapedia_0.1.0_aarch64.dmg --intel-dmg Phrapedia_0.1.0_x64.dmg
# sha256 を直接渡す場合は --arm-sha / --intel-sha。別ファイルは --cask <path>
```

## tap リポジトリの作り方

`brew tap luangpho/tap` は GitHub の **`luangpho/homebrew-tap`**（`homebrew-` 接頭辞が必須）を参照します。このリポジトリには含まれないので、別途作成します。

1. GitHub で `luangpho/homebrew-tap` を作成する（public。README だけで可）。
2. `Casks/` ディレクトリを作り、`packaging/homebrew/Casks/phrapedia.rb` をコピーして push する（以降は release ワークフローが更新する）。
3. 自動更新用の認証を用意する（どちらか。**推奨は deploy key**）。Settings → Secrets and variables → Actions に登録する。
   - **`HOMEBREW_TAP_SSH_KEY`（推奨）**: `luangpho/homebrew-tap` に write 権限の deploy key を作る（`ssh-keygen -t ed25519 -f tap_key -N ""`。公開鍵を homebrew-tap の Settings → Deploy keys に "Allow write access" 付きで登録）。秘密鍵（OpenSSH 形式）を `HOMEBREW_TAP_SSH_KEY` として登録する。
   - `HOMEBREW_TAP_TOKEN`: `luangpho/homebrew-tap` への書き込み権限を持つ Personal Access Token（fine-grained: 対象リポジトリ `homebrew-tap`、Contents: Read and write）を `HOMEBREW_TAP_TOKEN` として登録する。
4. 動作確認: `brew tap luangpho/tap && brew install --cask phrapedia`。

## 署名・notarization の設定

**Apple の署名・notarization は任意**です。既定は ad-hoc 署名（`src-tauri/tauri.conf.json` の `bundle.macOS.signingIdentity = "-"`）で、Apple Developer Program のアカウントは不要です。Apple Developer のアカウントがあれば、リポジトリ secret を登録するだけで release ワークフローが署名と notarization を行います。**未設定の場合はスキップ**され、ad-hoc 署名のビルドになります。

| Secret | 内容 |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application 証明書（.p12）を base64 化した文字列（`base64 -i cert.p12 \| pbcopy`） |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 を書き出した時のパスワード |
| `APPLE_SIGNING_IDENTITY` | 例: `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | notarization に使う Apple ID |
| `APPLE_PASSWORD` | その Apple ID の app 用パスワード（appleid.apple.com で発行） |
| `APPLE_TEAM_ID` | Team ID |

詳細は [Tauri: macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/) を参照。署名なしのビルドを入れた場合、初回は「右クリック → 開く」か、`xattr -dr com.apple.quarantine /Applications/Phrapedia.app` で隔離属性を外します。

## トラブルシューティング

| 症状 | 対処 |
|---|---|
| **CLI が見つからない**（`CLI not found`） | `agy --version` が通るか確認（インストール: `curl -fsSL https://antigravity.google/cli/install.sh \| bash`）。PATH が通らない環境（サービス起動など）では 設定 →「コマンド」に絶対パス（`which agy` の結果、通常 `~/.local/bin/agy`）を入れる |
| `agy` が終了コード 1 で失敗 | 未ログインの可能性。ターミナルで一度 `agy` を起動して Google にサインインするか、`GEMINI_API_KEY` ＋ `~/.gemini/antigravity-cli/settings.json` の `"modelProvider": "gemini"` を設定 |
| `agy` が終了コード 3 | モデル／エージェント側のエラー、または `--print-timeout` による打ち切り。部分的な応答があればそのまま使われる |
| 旧 Gemini CLI が認証エラー（終了コード 41） | `GEMINI_API_KEY` をアプリの起動環境の環境変数に設定するか、ターミナルで一度 `gemini` を起動してログイン |
| **Web 調査がタイムアウト** | 設定 →「タイムアウト」を延ばす（例: 300000）。モデルを `flash` にすると速くなる。同じ質問はキャッシュされる |
| **チャットが「Claude Code CLI（claude）が見つかりません」** | `claude --version` が通るか確認。PATH が通らない環境（Finder 起動のデスクトップアプリ、サービス起動）では 設定 →「AI」→「コマンド」に絶対パス（`which claude` の結果、通常 `~/.local/bin/claude`）を入れる。チャットは Claude 専用で Gemini では代替できません |
| チャットが「ログインしていないようです」 | ターミナルで `claude` を一度起動してログイン（Pro / Max など）。別マシンでは各マシンでログインが必要 |
| 検索が壊れた / 履歴が変 / 旧データが見えない | `<app data dir>/state/phrapedia.sqlite`（と `-wal` / `-shm`）を消すと再生成されます（失われるのは履歴・検索インデックス・調査キャッシュだけで、レコードと一次資料は無事）。旧版の PGlite（`.phrapedia/`）のチャット履歴は移行されません |
| 画面に「データセットパスが使えません」「書き出し先パスが使えません」 | 設定画面のパスを確認（絶対パス・存在・ディレクトリ・書き込み権限）。直すまでレコード・一次資料・Git は使えません |
| 一次資料が検索に出ない | パス（設定 → 一次資料フォルダ）を確認。`.` で始まるフォルダは対象外。PDF は本文検索非対応 |
| 同期でエラー | ターミナルで `git status` / `git pull --rebase` を実行して状態を確認。リモートの認証（SSH 鍵 / トークン）を確認 |
| MCP クライアントが繋がらない | HTTP の `/mcp` は廃止済みです。stdio の `phrapedia mcp` を `--config-dir` / `--data-dir` 付きで起動する設定にしているか、バイナリのパスが正しいか確認（[MCP で使う](#mcp-で使う)）。標準出力は MCP 専用なのでログは標準エラーに出ます |

## ロードマップ

- **プラクルアン（お守り）** — `data/amulets/` の本格対応（型・素材・製作年・鑑定ポイントの項目、高僧・寺院との関連付け、タブ追加）
- **画像管理** — レコードへの画像添付、一次資料画像のギャラリー、キャプションと出典の管理
- **認証** — 複数人での利用・リモート公開のためのログインと権限（MCP エンドポイントの保護を含む）
