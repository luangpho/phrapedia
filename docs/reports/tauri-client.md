# Tauri クライアント化 レポート

## 実装 worker (tauri-app)

### 変更内容
- `src-tauri/`: Tauri v2 シェル。Rust が `.output/server` を Node sidecar として `127.0.0.1` の空きポートで起動し、終了時にプロセスグループごと kill。`PHRAPEDIA_PARENT_PID` による自己終了ウォッチドッグ（`server/plugins/parent-watch.ts`）を併用。
- Nuxt: `PHRAPEDIA_TAURI=1` で SPA 出力（`.output-frontend`）。CLI の `dev` / `start` / Render は無変更（opt-in）。
- 設定: app config dir の `settings.json` にデータセットパス・書き出し先パスを保存（既定の書き出し先 = データセットパス）。Tauri command で読み書き・検証、`tauri-plugin-dialog` でフォルダ選択。sidecar へは環境変数 `PHRAPEDIA_DATASET_PATH` / `PHRAPEDIA_EXPORT_PATH`。（データセットパス・書き出し先とも root。直下に data/<kind>/ と sources/、PHRAPEDIA_ROOT と同配置）
- server: `paths.ts` / `settings.ts` / `entities.ts` / `git.ts` に受け口を追加。不正パスは 409 を返し、プロセスは落ちない。`/api/health` に検証結果を出す。
- UI: `app/pages/settings.vue` に 2 パス設定、エラーバナー。
- git 同期の規則: 書き出し先を git 対象（repo の時のみ有効、それ以外は「Git 管理外」）。読み出しは書き出し先優先→データセット fallback。CLI（2 パス未指定）は従来動作。詳細は `docs/ARCHITECTURE.md`。
- scripts: `npm run tauri:dev` / `npm run tauri:build`。version は 0.1.0 のまま。

### テスト結果
- `npm ci` OK / `npm test` 57 passed（`tests/path-validation.test.ts` 含む）/ `npm run typecheck` exit 0
- `npm run generate:tauri`（nuxt generate）OK
- `cargo check` / `cargo clippy -- -D warnings` クリーン / `cargo test` 11 passed（設定 roundtrip・検証・破損ファイル、sidecar 起動停止・グループ kill・早期終了・Node 無し等）
- 手動 E2E（xvfb でリリースバイナリ起動）:
  - 実在パス（root 直下に data/monks と sources/、書き出し先は別 root）: `/api/health` が両パス ok、`/api/entities/monks` が両方の内容を 200 で返す。CLI 配置（PHRAPEDIA_ROOT/data）も従来どおり 200
  - 存在しないパス: アプリ・sidecar は起動、health がパスごとのエラーを返し、entities は 409。アプリ終了後に sidecar・ポートとも残存なし

### Linux build
- `npm run tauri:build` 成功: deb (11.6 MiB) / rpm (11.8 MiB) / AppImage (85.5 MiB)。webkit2gtk 等は導入済みで追加 apt なし。
- macOS バイナリは未作成（GitHub Actions 担当）。

### 未解決
- Node は `PATH` 上の `node` 依存（Homebrew 前提）。Node 不在時の挙動は cargo test（`missing_entry_is_an_error_not_a_panic` 等）の範囲のみ検証。
- GUI（WebView 内の操作）の目視確認は未実施。xvfb での起動と API 応答のみ確認。

## 実装 worker (brew-release)

### 変更内容
- `.github/workflows/release.yml`: tag `v*` push で macOS aarch64 / x86_64 を tauri-action でビルドし GitHub Release に `.dmg` を添付。`.app.tar.gz` はワークフロー内で `tar` して `gh release upload`。`APPLE_*` secret が空なら署名・notarization はスキップ（tauri-action は空文字を未設定扱い）。`cask` ジョブは `HOMEBREW_TAP_SSH_KEY`（deploy key、推奨）か `HOMEBREW_TAP_TOKEN` があるときだけ dmg を取得 → sha256 を埋めて `luangpho/homebrew-tap` の `Casks/phrapedia.rb` へ push。
- `packaging/homebrew/Casks/phrapedia.rb`: `depends_on formula: "node"`、`arch arm:/intel:` で URL と sha256 を切替。成果物名 `Phrapedia_<version>_{aarch64,x64}.dmg`（tauri-bundler の dmg 命名規則: `{productName}_{version}_{arch}`, x86_64 は `x64`）。sha256 は 0 埋めのプレースホルダ。
- `packaging/homebrew/update-cask.mjs`: `--version` と `--arm-dmg/--intel-dmg`（または `--arm-sha/--intel-sha`）で書き換え。入力検証と書き換え後の自己検査あり。
- README: Homebrew インストール / リリース手順 / tap リポジトリの作り方 / 署名・notarization / Tauri 設定項目 / 開発コマンドを追加。CLAUDE.md: tauri:dev / tauri:build / 設定項目 / 配布を追加。
- `src-tauri/` `server/` `app/` は未変更。version は 0.1.0 のまま。

### 確認結果
- actionlint 1.7.12（GitHub release のバイナリ。go が PATH に無く `go install` 不可）: `.github/workflows/release.yml` エラー 0。
- cask 構文: ruby が無く apt も不可のため、`@ruby/wasm-wasi`（npm）上の `RubyVM::InstructionSequence.compile` で確認。cask は通り、意図的に壊したファイルは SyntaxError で失敗。`brew audit` は未実施（macOS / brew なし）。
- update-cask.mjs: ダミー dmg / ダミー sha / 不正 version で実行。version・sha256 が書き換わり、sha は `sha256sum` と一致。不正入力は exit 1。
- `npm ci` OK / `npm test` 57 passed。

### 未解決
- ワークフロー実走は未実施（macOS runner が必要）。dmg 名はソース（tauri-bundler `dmg/mod.rs`）で確認したのみで、実ビルド出力との一致は初回タグで要確認。
- `.app.tar.gz` は tauri-action ではなく自前で作成（updater 鍵なしだと tauri は生成しないため）。
- 初回リリース前に `luangpho/homebrew-tap` の作成と `HOMEBREW_TAP_SSH_KEY`（推奨）または `HOMEBREW_TAP_TOKEN` の登録が必要（README 参照）。署名 secret 未設定だと Gatekeeper 警告あり。
- GUI 起動のアプリはシェル環境変数（`NUXT_ANTHROPIC_API_KEY` 等）を引き継がない可能性。sidecar は Rust 側の env を継承するだけで `.env` は読まない。実機（macOS）で要確認。

## レビュー (review worker)

対象: `claude/thai-monks-temples-db-6j0hoy..feat/tauri-client` 全体。データセットパス仕様は A 案で確定（master-cap 確定: root 直下に `data/<kind>/` と `sources/`、`PHRAPEDIA_ROOT` と同配置。書き出し先も同配置・既定は同一。`dataDir` / `sourcesDir` 設定は従来どおり有効）。B 案（data/ 相当を直置き）は不採用で、途中の A↔B 往復は全て A に戻した。

### 指摘一覧
| # | severity | 内容 | 修正 |
|---|---|---|---|
| 1 | medium | パス設定が相対パス / `~` を受理。Rust 側は app の cwd、sidecar は別 cwd で解決するため検証と実体がずれる | 済 `4d01e9b`（絶対パス必須、テスト追加） |
| 2 | low | sidecar 起動判定が「ポートに繋がる」だけ。`free_port()` と listen の間に他プロセスが同ポートを取ると false ready になる | 済 `09ca423`（接続成功後に子プロセス生存を再確認、テスト追加） |
| 3 | low | リリースビルドでも CORS 許可 origin に `http://localhost:1420`（dev サーバ）が含まれる | 済 `09ca423`（debug ビルドのみ） |
| 4 | info | アプリが SIGKILL された場合、`parent-watch` で node は最大 3 秒で終了するが、node の子（`agy` 等）は残りうる | 未修正（影響小。下記） |
| 5 | info | `safeJoin` は字句的検証のみ。`sources/` 内の symlink が外を指す場合は辿る（データセットはユーザー選択パスのため許容範囲） | 未修正（server/ は最小修正方針） |
| 6 | info | CSP の `script-src` に `'unsafe-eval'`（Nuxt dev 用の可能性）。本番で必要かは未検証 | 未修正 |
| 7 | medium | A↔B の往復で README・設定画面説明・テスト・report の記述が A とずれうる状態になった | 済（A に統一。`tests/layout.test.ts` は A 配置 4 件、README / ARCHITECTURE / CLAUDE.md / report は A で一貫を確認） |

観点別の結論: slug は `assertSlug`（`/` `\` `.` 先頭を拒否）、`sources/` は `safeJoin` で検証済み。設定ファイル破損は既定値 + 警告で起動継続（テストあり）。書込不可の書き出し先は実書込プローブで検出。Node 不在は起動失敗としてエラーバナー表示。cask / workflow / `tauri.conf.json` は整合（`productName` Phrapedia、version 0.1.0、identifier `jp.grrow.phrapedia`、成果物 `Phrapedia_<version>_{aarch64,x64}.dmg`、sha256 は `update-cask.mjs` が dmg から計算して埋める、arch 切替は `arch arm:/intel:`）。`render.yaml` / `scripts/render-start.mjs` は無変更、CLI 起動（env 未設定）は従来どおり動作。

### テスト結果
- `npm ci` OK / `npm test` 61 passed（`tests/layout.test.ts` 4 件追加）/ `npm run typecheck` exit 0（vue-tsc の `vue-router/volar` 解決警告のみ）/ `npx nuxt generate`（`generate:tauri`）OK
- `cargo check` / `cargo clippy -- -D warnings` クリーン / `cargo test` 12 passed（修正で 1 件追加）
- `npm run tauri:build` 成功（Linux: deb / rpm / AppImage）
- `actionlint .github/workflows/release.yml` エラー 0（初回確認時。以降 release.yml は無変更。バイナリ再取得は権限拒否のため再実行せず）
- 手動（`.output/server` を env で直接起動）: 2 パス指定（各 root。データセット側に `data/monks/` と `sources/`）で `/api/health` ok、`/api/entities/monks` 200、POST で作成したレコードは書き出し先 `<export>/data/monks/` にのみ出力（データセット側は無変更）。存在しないパスでもプロセスは落ちず、health が両パスのエラーを返し entities は 409。env 未設定（CLI 相当）も health / monks 200。終了後にプロセス・ポート残存なし

### 未解決
- macOS 実機・ワークフロー実走・`brew audit` は未確認（Linux 環境のため）。dmg 名は初回タグで要確認。
- #4〜#6（上記）。GUI 操作の目視確認も未実施。review worker の後始末: `rm -rf` / 一部 `find -delete` は権限で拒否されるため、`w-review/` 配下の clone・生成物・`/tmp/rv-*.log` は残存しうる。

## docs 文言・不要ファイル整理（仕上げ）

- README の設定リファレンス表（データセットパス / 書き出し先パス）を A 案（直下に `data/<kind>/` と `sources/` を持つ root）の文言に修正。
- `data/.{0,4}相当|直下に.{0,6}monks` を README / CLAUDE.md / docs / app で全数確認。残りは本 report の A 案文脈（19 行目）のみ。
- 既定値（アプリデータ dir 配下 `dataset`、書き出し先 = データセットと同じ）は `src-tauri/src/lib.rs` の `resolve_paths` と一致。
- `src-tauri/icons/android` `ios` を削除（`tauri.conf.json` の `bundle.icon` は参照なし）。
- `src-tauri/gen/` は `tauri build` / `cargo check` が自動生成する schema のため `.gitignore` に追加し追跡解除（残す必要なし）。
- 検証: `npm test` 57 passed / `npm run typecheck` exit 0 / `cargo check` 成功。

## docs-review 検証（HEAD 7ab4e13、別 clean clone）

- 文言: `data/.{0,4}相当|直下に.{0,6}monks`、`相当`、`data-equivalent` を README / CLAUDE.md / docs / app で確認。残りは本 report の A 案文脈（19 / 72 / 81 行目）のみ。B 案の残存なし。
- 既定値: README 表と UI（PathsCard.vue）の記述は `lib.rs` の `resolve_paths`（アプリデータ dir 配下 `dataset`）と `settings.rs` の `effective_export`（空 = データセットと同じ）に一致。
- clean clone で `npm ci` / `npm test` 57 passed / `npm run typecheck` exit 0（vue-router/volar 警告のみ）/ `cargo check` 成功 / `npm run tauri:build` 成功（deb / rpm / AppImage）。ビルド後 `git status` は clean（`src-tauri/gen` は ignore され再生成される）。
- 削除した `icons/android` `ios` と `src-tauri/gen` への参照は `.github` / `tauri.conf.json` / capabilities に無し。追跡ファイルの欠けなし。
- 未実施: `actionlint .github/workflows/release.yml`。本環境に actionlint が無く、docker daemon も無い。バイナリ取得は権限で拒否されたため未実行。

## cask ジョブの SSH deploy key 対応

- `release.yml` cask ジョブ: gate は `HOMEBREW_TAP_TOKEN` / `HOMEBREW_TAP_SSH_KEY` のどちらかが非空で enabled、両方空ならスキップ。
- push: `HOMEBREW_TAP_SSH_KEY` があれば優先。秘密鍵を `$RUNNER_TEMP` に 0600 で書き、`ssh-keyscan` の known_hosts と `GIT_SSH_COMMAND`（IdentitiesOnly=yes / StrictHostKeyChecking=yes）で `git@github.com:luangpho/homebrew-tap.git` を clone・push。鍵が無く token があれば従来の https。
- 鍵・known_hosts は `if: always()` の最終ステップで削除。secret は env 経由、`printf '%s\n'` で書き、`set -x` / echo は使わない。cask 生成・commit は変更なし。
- README の tap 手順に deploy key の作り方を追記（推奨）。
