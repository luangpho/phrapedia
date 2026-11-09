# 配布・release を bantou desktop の方式に揃える

branch: `fix/dist-like-bantou`。目的は `brew install` → 起動、`brew upgrade`、release を、PAT 発行・Apple 署名・手作業の版上げ無しで回せること。cask（`packaging/homebrew/Casks/phrapedia.rb`）は変更していない。

## 作業内容

- 署名: `src-tauri/tauri.conf.json` の bundle に `macOS.signingIdentity = "-"`（ad-hoc）を明示。
- アプリ内更新（tauri updater）の配線:
  - `tauri-plugin-updater` / `tauri-plugin-process` を Cargo.toml・`lib.rs`・capabilities に追加。npm 側の対応パッケージも追加。
  - フロントは `app/composables/useUpdater.ts`。起動時に 1 回 `check()` し、更新があれば確認 → `downloadAndInstall` → `relaunch`。失敗は黙って無視。Tauri デスクトップ時のみ動く（`app/plugins/tauri.client.ts` から呼ぶ）。
  - `lib.rs` は `plugins.updater` の設定がある時だけ updater plugin を登録する。設定が無いまま登録すると plugin が build 時に panic するため。
  - updater 設定（endpoints・pubkey・`createUpdaterArtifacts`）は `src-tauri/tauri.updater.conf.json`。pubkey は空文字のプレースホルダ。鍵は master-cap 側で後から入れる。
- `.github/workflows/release.yml`: secret `TAURI_SIGNING_PRIVATE_KEY` が空でなく、かつ pubkey が空でない時だけ `--config src-tauri/tauri.updater.conf.json` と署名 env を渡す（`$GITHUB_ENV` 経由。空 secret は export しない）。鍵が無い時は従来どおり dmg だけ。cask job と `HOMEBREW_TAP_SSH_KEY` 対応は変更なし。
- `.app.tar.gz` の衝突整理: updater 有効時は tauri-action が `Phrapedia_<ver>_<arch>.app.tar.gz` を `.sig` 付きで上げる。既存の「Attach .app.tar.gz」step が同名を `--clobber` で上げ直すと `.sig` と中身が食い違うので、有効時はスキップ（`if: env.UPDATER_ENABLED != 'true'`）。鍵なし時は従来どおり。
- `scripts/release.sh <version>`（`--dry-run` あり）: bash 3.2 で動く書き方（mapfile 不使用、版の書き換えは node）。検査 → 新規 clone → `npm ci`/test/typecheck → 5 ファイルの version を揃えて 1 commit（各ファイルの変更行数をガード）→ 既定 branch push → tag push → `gh run watch` → `gh release view` と tap の cask version 表示 → 一時 dir 削除。`~/.config/bantou/release-trailer` があれば commit に付ける。`PHRAPEDIA_REPO_URL` で clone 元を差し替え可（検証用）。
- README の「リリース手順」「署名・notarization」と CLAUDE.md の「配布」を更新。「Homebrew でインストール」節は触っていない。

## 確認結果

- `npm ci && npm test`: 61 passed。
- `npm run typecheck`: 終了コード 0。ただし vue-tsc が `vue-router/volar/sfc-route-blocks` の解決失敗を警告として出す（今回の変更とは無関係。release.sh の dry-run でも同じ出力）。
- `cargo check` / `cargo clippy -- -D warnings` / `cargo test`: 通過（15 passed）。
- `npm run tauri:build`（Linux、最終コードで再実行）: deb / rpm / AppImage 成功（updater plugin 組み込み、updater 設定なし）。xvfb で 20 秒起動し panic せず生存（timeout で終了）。`check()` の失敗は `useUpdater.ts` の try/catch で握り潰す作りだが、WebView 内の実行時挙動（plugin 未登録時の reject）は観測していない。
- `scripts/release.sh --dry-run 0.1.1`: ローカルの bare repo を `PHRAPEDIA_REPO_URL` で clone 元にし、`gh` は stub（`auth status` のみ成功）で実行。rc=0、5 ファイルの変更行数ガード通過、`git push --dry-run` のみで bare repo の ref は実行前後で不変。実 remote には触れていない。この確認中に「dry-run の tag push が、ローカルに tag が無いため失敗する」不具合を見つけて修正済み（`$SHA:refs/tags/$TAG` を dry-run push）。
- release.yml を js-yaml で parse（jobs: build, cask）。updater 有効・無効の両分岐を run 部の抽出で確認。actionlint は手元に無く省略。

## 残課題

- `latest.json` の並列 merge 競合は build job に `strategy.max-parallel: 1` を入れて回避（aarch64 → x64 の直列、所要時間は増える）。
- 実際の GitHub Actions 上で、matrix 2 本の `latest.json` に `darwin-aarch64` / `darwin-x86_64` が両方載ることは、次の release で要確認（tauri-action の仕様に基づく想定で、CI 上では未検証）。
- macOS 実機での release.sh 実行（bash 3.2）とアプリ内更新の動作は未検証（Linux のみ確認）。
- `npm run typecheck` の vue-router volar 警告は別件。

## 追加作業（cask の bantou 同形化・updater 公開鍵）

- cask: `packaging/homebrew/Casks/phrapedia.rb` に `auto_updates true` と `preflight_steps`（`xattr -dr com.apple.quarantine` のみ。再署名しない）を追加。arch arm/intel・`depends_on formula: "node"`・zap は据え置き。
- `tests/update-cask.test.ts` を追加: 新テンプレートで `update-cask.mjs` が version / sha256 (arm・intel) を書き換え、preflight_steps 等が残ることを確認。
- `src-tauri/tauri.updater.conf.json` の pubkey に公開鍵を設定。release.yml は `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` を空文字で明示（secret は参照しない）。
- README「Homebrew でインストール」: 右クリックで開く・xattr 手打ちの案内なし。cask が隔離属性を外す旨に変更。
- 確認: `npm test` 63 passed、typecheck 終了コード 0、js-yaml parse と run 部抽出で updater 有効（`--config` と env）/ 無効の両分岐を確認。ruby は無く `ruby -c` は省略。


## 追加作業（latest.json 競合対策・README の鍵節）

- release.yml の build job に `strategy.max-parallel: 1` を追加。
- README「アプリ内更新（tauri updater）の鍵」を実態に合わせて書き直し（鍵は生成・secret 登録済み、pubkey は commit 済み、password 無し、「鍵登録までは無効」の文を削除）。
- 確認: js-yaml parse と run 部抽出で updater 有効・無効（secret 空 / pubkey 空）の両分岐を再確認。

## レビュー（cask-review）

- 確認: cask は bantou の `packages/desktop/Casks/bantou.rb` と同形（preflight_steps は app の後・zap の前、auto_updates true、arch arm/intel、depends_on formula "node"、zap あり）。pubkey は指定値と完全一致し、base64 decode で minisign public key（key ID 7402570F8B336415）。release.yml は js-yaml で parse でき、PASSWORD は `""` 固定で secret 参照なし。run 部を実行し、鍵あり（`--config` と KEY / 空 PASSWORD を `$GITHUB_ENV` へ）・鍵なし（何も書かない）の両分岐を確認。cask job は a882ab1 から無変更。秘密鍵・token の混入なし。version は 0.1.0 のまま。
- 指摘: README「更新方法」と「アプリ内更新の鍵の登録」節に、pubkey 未登録・password 前提の古い記述が残っていた。同内容の修正が並行 commit（064dd0e）で先に入っていたため、README はそちらを採用し、現状（鍵は登録済み・パスワード無し・「鍵を作り直す場合」あり・「鍵登録までは無効」の文なし）を確認した。追加の README 変更なし。
- 報告のみ（未変更）: 「署名・notarization の設定」節末尾に「右クリック → 開く」/ xattr の案内がある。cask 経由でない dmg 直接導入向けなので残した。
- 上の「残課題」のうち updater 鍵の項は解消済み。`latest.json` に両 platform が載るかは、初回 release 後に要確認のまま（max-parallel: 1 で競合は回避済みの想定）。
- テスト: `npm ci && npm test` 63 passed、`npm run typecheck` 終了コード 0。ruby / actionlint は手元に無く省略。

## レビュー (review worker)

対象: PR #3（`fix/dist-like-bantou` → `claude/thai-monks-temples-db-6j0hoy`）、commit 0886e62 / 8538f56 / 90b7d27 / a882ab1。bantou（`packages/desktop`）と比較した。

### 指摘一覧
| # | severity | 内容 | 修正 |
|---|---|---|---|
| 1 | high | 更新確認ダイアログが `window.confirm` だった。wry の WKUIDelegate は JS confirm panel を実装しておらず（wry 0.57.1 のソースで確認）、macOS の WKWebView では Cancel 扱いになり、更新があっても黙って何も起きない恐れがある（実機未検証） | 済: `@tauri-apps/plugin-dialog` の `ask` に変更、capabilities に `dialog:allow-message` を追加 |
| 2 | low | `process:default` は `allow-exit` も含む。必要なのは再起動のみ | 済: `process:allow-restart` に絞り、Rust の設定テストも更新 |
| 3 | medium | `latest.json` の競合（下記）。matrix 2 本が同時完了すると片方の platform が消えうる | 済（cap が 26ca064 で対策 A `max-parallel: 1` を適用。review worker は適用していない） |
| 4 | info | 既存の `app/pages/[kind]/[slug].vue:41` と `app/utils/unsaved.ts:16` も `window.confirm`。#1 と同じ理由で macOS では常に Cancel 扱いになり、未保存の編集を破棄して離れる操作ができない可能性がある（本 PR 範囲外・実機未検証） | 未修正（別件として cap へ） |
| 5 | info | updater 設定は bantou が本体の `tauri.conf.json` に持つのに対し、こちらは overlay（`tauri.updater.conf.json`）。pubkey が空の間は `createUpdaterArtifacts` も外れるため、鍵無しで release が通る設計として妥当 | 変更なし |

### 要件ごとの確認
- `signingIdentity "-"`: bantou と同じ（`bundle.macOS.signingIdentity`）。
- updater 配線: plugin 登録は `plugins.updater` がある時のみ（`app.config().plugins.0.contains_key("updater")`）。設定無しで登録すると plugin の config 読み込みが失敗して起動が止まるため妥当。設定無しの Linux バイナリを xvfb で起動し panic しないことを dist worker が確認済み。`check()` の reject は try/catch で握りつぶす。
- release.yml: js-yaml で parse（jobs: build, cask）。`Export updater signing env` の run 部を抽出して 4 ケース実行（鍵なし / pubkey なし / 両方なし → `GITHUB_ENV` に何も書かず従来どおり、両方あり → `UPDATER_ENABLED` / `UPDATER_ARGS` / 署名 env を複数行の鍵も壊さず出力）。`Attach .app.tar.gz` の `if: env.UPDATER_ENABLED != 'true'` は妥当: tauri-action は `Phrapedia_<ver>_<arch>.app.tar.gz` を `.sig` 付きで上げる（`utils.ts` の命名と一致）ため、手で tar し直して `--clobber` すると `.sig` と中身が食い違う。cask job は無変更。
- `scripts/release.sh`: bash 3.2 非互換（mapfile / 連想配列 / `${var,,}` / `&>>` 等）なし。引数検査、変更行数ガード、trap による後始末を確認。bare repo + gh stub で `--dry-run 0.1.1` を実走: rc=0、5 ファイルを 1 commit、`git push --dry-run` のみで bare repo の ref は実行前後で不変、一時 dir は消える。
- cask: レビュー開始後に dist worker が eb77a75 で `auto_updates true` と `preflight_steps`（`xattr -dr com.apple.quarantine`）を追加した。Homebrew 本体のソース（`Library/Homebrew` の `install_steps.rb`、`docs/Cask-Cookbook.md`）で、`preflight_steps` が現行の正式な stanza であること、`run "xattr", args: [...]` の args 内 `{{staged_path}}` が install 時に展開されること、sandbox が caskroom 配下への書き込みを許可することを確認した。名前を指定した `brew upgrade --cask phrapedia` は auto_updates でも対象になる（`cask/upgrade.rb`）。ただし `preflight_steps` が無い古い Homebrew では cask の読み込みに失敗しうる（導入された版は未確認。`brew update` が前提）。version は 0.1.0 のまま、`update-cask.mjs` のテストが追加されて通る。pubkey は d066264 で公開鍵が設定された（公開鍵のみ）。秘密鍵・token・password の混入なし（パターン検索。release.yml は鍵 password を空文字で明示）。
- bantou との差: bantou は単一 platform（aarch64）をローカルで署名ビルドして `make-latest-json.sh` で `latest.json` を作り、npm publish と手元アプリの入れ替えまで行う。phrapedia は CI（macOS matrix 2 本）がビルドし、`latest.json` は tauri-action が作る。release.sh はビルドせず版上げ・push・tag・workflow 待ちまで。README は新しい手順に更新済みで、「Homebrew でインストール」節は触っていない。

### latest.json 競合の見解（tauri-action の `src/upload-version-json.ts` を確認）
- 実在する。各 job の流れは「release の assets を list → 既存 `latest.json` を download → `platforms` を merge → 既存 asset を delete → upload」で、job 間のロックが無い。2 本が近い時刻に走ると、(a) 両方が古い `latest.json` を読んで後勝ちになり、片方の platform が消える（エラーにならず気づけない）、(b) 片方が delete した後に他方が download して 404 で失敗する。(b) は tauri-action 自身が 1 回 retry するが、(a) は retry の対象外。
- 発生確率: 両 job の完了時刻が近いほど高い。aarch64 と x64（クロスコンパイル）の所要時間は近い見込みで、無視できない。
- 対策 A（`build` job に `strategy.max-parallel: 1`）: 2 本目は 1 本目の完了後に list するため、必ず 1 本目の `darwin-aarch64` を merge できる。`fail-fast: false` なので 1 本目が失敗しても 2 本目は走る。効果は確実で変更は 1 行。代償は所要時間が直列化で約 2 倍になること。cask job は `needs: build` なので影響しない。
- 対策 B（後続 job で組む）: build job の tauri-action に `uploadUpdaterJson: false` を渡し、`needs: build` の job が release から 2 つの `.sig` を取得して node で `latest.json`（`darwin-aarch64` / `darwin-x86_64`）を作り、`gh release upload --clobber` する。時間は増えず競合も原理的に無いが、workflow に job と数十行のスクリプトが増え、url / signature の組み立てを自前で保守する。bantou の `make-latest-json.sh` を 2 platform に拡張する形になる。
- pubkey が設定済みになったため、`TAURI_SIGNING_PRIVATE_KEY` を登録した時点でこの競合は現実のリスクになる。
- 推奨: 初回は A（最小変更）。時間が問題になったら B。cap の判断で A が 26ca064 で適用された。鍵登録後の最初の release で `latest.json` に 2 platform が載ることを確認すること。

### テスト結果
- `npm ci` OK / `npm test` 63 passed（rebase 後。cask テスト追加分を含む）/ `npm run typecheck` exit 0（vue-router/volar の解決警告のみ）。
- `cargo check` / `cargo clippy -- -D warnings` クリーン / `cargo test` 15 passed。
- `npm run tauri:build`（Linux、修正後）: deb / rpm / AppImage 成功（capabilities の権限名もここで検証される）。
- actionlint は手元に無く省略。

### 未解決
- macOS 実機での `ask` ダイアログ表示・アプリ内更新・release.sh（bash 3.2）は未検証。
- #4 の別件対応。
