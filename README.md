# Phrapedia

タイの **高僧・寺院・プラクルアン（お守り）** の情報を、AI と一緒に調べて **Markdown ファイル** として蓄積するローカルファースト型の知識ベースです。

- 正本は `data/<種類>/<slug>.md`（YAML frontmatter + Markdown）。Git で履歴管理し、GitHub に同期できます。
- 手元の **一次資料**（`sources/` のメモ・画像）を最優先し、足りない部分だけを **Gemini CLI による Web 調査** で補います。Web 由来の情報は常に「未確認」として扱います。
- 同じツール群を **MCP サーバ**（`/mcp`）として公開しているので、Claude Code や Gemini CLI からも読み書きできます。

> 内部構成の詳細は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) を参照してください。

---

## 目次

1. [機能](#機能)
2. [必要なもの](#必要なもの)
3. [クイックスタート](#クイックスタート)
4. [情報の優先順位](#情報の優先順位)
5. [画面の流れ](#画面の流れ)
6. [設定リファレンス](#設定リファレンス)
7. [Git 同期](#git-同期)
8. [MCP で使う](#mcp-で使う)
9. [開発コマンド](#開発コマンド)
10. [トラブルシューティング](#トラブルシューティング)
11. [ロードマップ](#ロードマップ)

---

## 機能

| 機能 | 内容 |
|---|---|
| レコード管理 | 高僧・寺院のレコードを一覧・閲覧・編集・削除。Markdown プレビュー付きエディタ |
| AI チャット | レコードごとのチャット窓。AI が一次資料を検索し、Web 調査で補い、**下書き** をエディタに反映（保存はユーザーが行う） |
| 一次資料 | 任意フォルダの Markdown / 画像を全文検索（タイ語・日本語・英語の部分一致）・プレビュー |
| Web 調査 | Gemini CLI（Google 検索グラウンディング）をサーバから実行。結果はキャッシュ |
| Git 同期 | `data/` だけをコミット → `pull --rebase` → `push`。保存ごとの自動コミット／自動プッシュも可 |
| MCP サーバ | `http://localhost:3000/mcp`（Streamable HTTP）。外部の AI エージェントから同じ操作が可能 |
| ダークモード | Nuxt UI のカラーモードに対応 |

## 必要なもの

- **Node.js 22 以上**
- **Gemini CLI**（Web 調査に使用）
  ```bash
  npm i -g @google/gemini-cli
  gemini --version
  ```
  認証はどちらか一方:
  - 環境変数 `GEMINI_API_KEY` を設定する（[Google AI Studio](https://aistudio.google.com/) で発行）
  - このマシンで一度 `gemini` を対話起動して **Google アカウントで OAuth ログイン** しておく（以後は資格情報が再利用されます）
- **Anthropic API キー**（チャット AI に使用）
  - 無い場合は 設定 →「チャットエンジン」を **Gemini CLI** に切り替えればチャットも Gemini で動きます（Gemini CLI モードはツール呼び出し無しの単発応答で、一次資料のテキストは自動で同梱されますが画像資料は渡せません。精度は Claude の方が安定します）。
- Git（同期を使う場合。GitHub の認証は通常の `git push` が通る状態にしておく）

## クイックスタート

```bash
git clone <このリポジトリ> phrapedia && cd phrapedia
npm install
cp .env.example .env      # NUXT_ANTHROPIC_API_KEY を記入（GEMINI_API_KEY も必要なら）
npm run dev               # http://localhost:3000
```

初回起動時に `.phrapedia/`（PGlite のデータベース。設定・チャット履歴・検索インデックス・Web 調査キャッシュ）が作られます。Git 管理外です。

サンプルとして以下のレコードと一次資料が同梱されています（いずれも `status: draft`、「要確認」箇所あり）。

| 種類 | レコード |
|---|---|
| 高僧 | `data/monks/somdet-to.md`（ソムデット・トー）, `data/monks/luang-pu-thuat.md`（ルアンプー・トゥアット） |
| 寺院 | `data/temples/wat-rakhang.md`（ワット・ラカン）, `data/temples/wat-chang-hai.md`（ワット・チャーンハイ） |
| 一次資料 | `sources/notes/somdet-to.md`, `sources/notes/wat-rakhang.md`（サンプル） |

本番ビルドで動かす場合:

```bash
npm run build
npm start            # = node --env-file-if-exists=.env .output/server/index.mjs（本番サーバは .env を自動では読みません）
```

別の場所から起動する場合は `PHRAPEDIA_ROOT=/path/to/phrapedia` を指定します。待ち受けは既定で全インターフェース（0.0.0.0）なので、
自分だけが使うなら `HOST=127.0.0.1 PORT=3000 npm start` のようにローカルに限定してください。

### セキュリティ上の注意

Phrapedia はログイン機構のない「自分の PC で動かすツール」です。

- 設定画面の値（Gemini CLI のコマンドパス、フォルダなど）はサーバ側でそのまま使われます。**信頼できる人だけが届くネットワーク**で動かしてください（`HOST=127.0.0.1` 推奨）。
- ブラウザからの API 呼び出しには同一オリジン・JSON 限定のチェック（CSRF 対策）がかかっています。
- API キーはブラウザへ渡さず、`.env` / 環境変数のみで扱います。
- アプリ内のチャット AI には Git 同期ツール（`sync_data`）を渡していません。GitHub への push は常に同期パネルからの操作です（自動プッシュ設定を除く）。

## 情報の優先順位

| 優先度 | 情報源 | 扱い |
|---|---|---|
| 1 | **一次資料**（`sources/` 配下、設定で変更可） | 確定情報。AI は必ず最初に検索・参照する |
| 2 | **Web 調査**（Gemini CLI） | **未確認**。一次資料に無い部分だけを補う。本文で「未確認」と明記 |

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

設定画面の値は PGlite（`.phrapedia/`）に保存され、`.env` の値より優先されます。項目は `shared/types/settings.ts` の `AppSettings` に対応します。

| 項目 | キー | 既定値 | 説明 |
|---|---|---|---|
| 一次資料フォルダ | `sourcesDir` | `sources` | 絶対パス、またはプロジェクトからの相対パス。`.env`: `NUXT_SOURCES_DIR` |
| データフォルダ | `dataDir` | `data` | `data/<種類>/<slug>.md` の置き場所。Git 同期するならリポジトリ内に。`.env`: `NUXT_DATA_DIR` |
| チャットエンジン | `ai.provider` | `anthropic` | `anthropic`（Claude）または `gemini-cli` |
| Claude モデル | `ai.anthropicModel` | `claude-sonnet-5-5` | `.env`: `NUXT_ANTHROPIC_MODEL` |
| 最大トークン | `ai.maxTokens` | `8192` | 1 回の応答の最大出力トークン |
| 追加の指示 | `ai.extraInstructions` | （空） | システムプロンプト末尾に追加される指示 |
| Gemini コマンド | `gemini.command` | `gemini` | 実行ファイル名または絶対パス。`.env`: `NUXT_GEMINI_COMMAND` |
| 追加引数 | `gemini.args` | `[]` | CLI に渡す追加引数（1 行に 1 つ） |
| Gemini モデル | `gemini.model` | （空 = CLI 既定） | `-m` に渡す（例: `flash`, `pro`, `gemini-2.5-pro`） |
| 承認モード | `gemini.approvalMode` | `auto_edit` | `default`（web_fetch 不可）/ `auto_edit`（推奨）/ `yolo`（シェル等も許可、非推奨） |
| タイムアウト | `gemini.timeoutMs` | `180000` | Gemini CLI 1 回あたりの上限（ミリ秒） |
| キャッシュ時間 | `gemini.cacheHours` | `24` | 同じ調査結果を再利用する時間。`0` で無効 |
| リモート | `git.remote` | `origin` | push / pull 先のリモート名 |
| ブランチ | `git.branch` | （空 = 現在のブランチ） | 同期するブランチ |
| 作者名 / メール | `git.authorName` / `git.authorEmail` | （空 = git の設定） | コミット作者（リポジトリローカル設定に書き込み） |
| 自動コミット | `git.autoCommit` | `false` | レコードの保存・削除ごとにコミット |
| 自動プッシュ | `git.autoPush` | `false` | 自動コミット後に push |

API キーは画面からは設定できません（ブラウザに渡さないため）。`.env` の `NUXT_ANTHROPIC_API_KEY`（または `ANTHROPIC_API_KEY`）、`GEMINI_API_KEY` を使います。そのほか `NUXT_STATE_DIR`（PGlite の場所、既定 `.phrapedia`）、`PHRAPEDIA_ROOT`（プロジェクトルート、既定はカレントディレクトリ）があります。

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

アプリ起動中は `http://localhost:3000/mcp`（Streamable HTTP, ステートレス）が MCP サーバになります。

### Claude Code

リポジトリ直下に `.mcp.json` を同梱しています（このフォルダで `claude` を起動すると読み込まれます）。

```json
{
  "mcpServers": {
    "phrapedia": {
      "type": "http",
      "url": "http://localhost:3000/mcp"
    }
  }
}
```

別のプロジェクトから使う場合は `claude mcp add --transport http phrapedia http://localhost:3000/mcp`。

### Gemini CLI

`~/.gemini/settings.json`（またはプロジェクトの `.gemini/settings.json`）:

```json
{
  "mcpServers": {
    "phrapedia": {
      "httpUrl": "http://localhost:3000/mcp"
    }
  }
}
```

### ツール一覧

| ツール | 説明 |
|---|---|
| `list_entities` | 保存済みレコード（monks / temples / amulets）の slug・名前・状態の一覧 |
| `get_entity` | 1 件のレコードの Markdown 全文（frontmatter 含む） |
| `save_entity` | レコードの作成・更新（Markdown 全文を渡す。slug 自動生成、自動コミット設定に従う） |
| `entity_template` | 新規レコード用のテンプレート（frontmatter + 見出し） |
| `list_sources` | 一次資料フォルダのファイル一覧 |
| `search_sources` | 一次資料の全文検索（部分一致、タイ語・日本語・英語） |
| `read_source` | 一次資料を読む（テキストは本文、画像は画像として返す） |
| `web_research` | Gemini CLI で Web 調査（結果は **未確認** 扱い、キャッシュあり） |
| `sync_status` | データフォルダの Git 状態 |
| `sync_data` | データフォルダをコミット → pull --rebase → push |

このほか、リソース `phrapedia://{kind}/{slug}`（レコード本文）と、プロンプト `research_record`（一次資料 → Web 調査 → 保存 の手順ガイド）を提供します。

## 開発コマンド

```bash
npm run dev         # 開発サーバ（http://localhost:3000）
npm run build       # 本番ビルド（.output/）
npm run typecheck   # nuxt typecheck
npm test            # vitest（tests/**/*.test.ts）
npm run smoke       # ビルド済みサーバを起動して REST / MCP の疎通確認（先に npm run build）
npm start           # 本番サーバ起動（.env を読み込む）
```

## トラブルシューティング

| 症状 | 対処 |
|---|---|
| **Gemini CLI が見つからない**（`Gemini CLI not found`） | `npm i -g @google/gemini-cli` を実行し、`gemini --version` が通るか確認。PATH が通らない環境（サービス起動など）では 設定 →「Gemini コマンド」に絶対パス（`which gemini` の結果）を入れる |
| Gemini が認証エラー（終了コード 41） | `GEMINI_API_KEY` を `.env` かシェルに設定するか、ターミナルで一度 `gemini` を起動してログイン |
| **Web 調査がタイムアウト** | 設定 →「タイムアウト」を延ばす（例: 300000）。モデルを `flash` にすると速くなる。同じ質問はキャッシュされる |
| **チャットが「API キーが設定されていません」** | `.env` に `NUXT_ANTHROPIC_API_KEY=sk-ant-...` を書いてサーバを再起動。無い場合は チャットエンジンを Gemini CLI に |
| API キーが無効（401） | キーの値・有効期限・組織の利用制限を確認 |
| **PGlite のエラー / 起動しない** | `.phrapedia/` は消しても再生成されます（設定・チャット履歴・キャッシュは失われ、レコードと一次資料は無事）。同じ `.phrapedia/` を 2 つのサーバで同時に開くと壊れることがあるので、開発サーバと本番サーバを同時に起動しない（どうしても必要なら `NUXT_STATE_DIR` を分ける） |
| 一次資料が検索に出ない | パス（設定 → 一次資料フォルダ）を確認。`.` で始まるフォルダは対象外。PDF は本文検索非対応 |
| 同期でエラー | ターミナルで `git status` / `git pull --rebase` を実行して状態を確認。リモートの認証（SSH 鍵 / トークン）を確認 |
| MCP クライアントが繋がらない | アプリ（`npm run dev`）が起動しているか、ポートが 3000 か確認。ブラウザで `/mcp` を開くとトップへリダイレクトされるのは正常 |

## ロードマップ

- **プラクルアン（お守り）** — `data/amulets/` の本格対応（型・素材・製作年・鑑定ポイントの項目、高僧・寺院との関連付け、タブ追加）
- **画像管理** — レコードへの画像添付、一次資料画像のギャラリー、キャプションと出典の管理
- **認証** — 複数人での利用・リモート公開のためのログインと権限（MCP エンドポイントの保護を含む）
