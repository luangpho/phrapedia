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
- `.github/workflows/release.yml`: tag `v*` push で macOS aarch64 / x86_64 を tauri-action でビルドし GitHub Release に `.dmg` を添付。`.app.tar.gz` はワークフロー内で `tar` して `gh release upload`。`APPLE_*` secret が空なら署名・notarization はスキップ（tauri-action は空文字を未設定扱い）。`cask` ジョブは `HOMEBREW_TAP_TOKEN` があるときだけ dmg を取得 → sha256 を埋めて `luangpho/homebrew-tap` の `Casks/phrapedia.rb` へ push。
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
- 初回リリース前に `luangpho/homebrew-tap` の作成と `HOMEBREW_TAP_TOKEN` の登録が必要（README 参照）。署名 secret 未設定だと Gatekeeper 警告あり。
- GUI 起動のアプリはシェル環境変数（`NUXT_ANTHROPIC_API_KEY` 等）を引き継がない可能性。sidecar は Rust 側の env を継承するだけで `.env` は読まない。実機（macOS）で要確認。
