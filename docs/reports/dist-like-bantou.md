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
