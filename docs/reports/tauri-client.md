# Tauri クライアント化 レポート

## 実装 worker (tauri-app)

### 変更内容
- `src-tauri/`: Tauri v2 シェル。Rust が `.output/server` を Node sidecar として `127.0.0.1` の空きポートで起動し、終了時にプロセスグループごと kill。`PHRAPEDIA_PARENT_PID` による自己終了ウォッチドッグ（`server/plugins/parent-watch.ts`）を併用。
- Nuxt: `PHRAPEDIA_TAURI=1` で SPA 出力（`.output-frontend`）。CLI の `dev` / `start` / Render は無変更（opt-in）。
- 設定: app config dir の `settings.json` にデータセットパス・書き出し先パスを保存（既定の書き出し先 = データセットパス）。Tauri command で読み書き・検証、`tauri-plugin-dialog` でフォルダ選択。sidecar へは環境変数 `PHRAPEDIA_DATASET_PATH` / `PHRAPEDIA_EXPORT_PATH`。（データセットパス = data/ 相当、直下に monks/ 等と sources/）
- server: `paths.ts` / `settings.ts` / `entities.ts` / `git.ts` に受け口を追加。不正パスは 409 を返し、プロセスは落ちない。`/api/health` に検証結果を出す。
- UI: `app/pages/settings.vue` に 2 パス設定、エラーバナー。
- git 同期の規則: 書き出し先を git 対象（repo の時のみ有効、それ以外は「Git 管理外」）。読み出しは書き出し先優先→データセット fallback。CLI（2 パス未指定）は従来動作。詳細は `docs/ARCHITECTURE.md`。
- scripts: `npm run tauri:dev` / `npm run tauri:build`。version は 0.1.0 のまま。

### テスト結果
- `npm ci` OK / `npm test` 57 passed（`tests/path-validation.test.ts` 含む）/ `npm run typecheck` exit 0
- `npm run generate:tauri`（nuxt generate）OK
- `cargo check` / `cargo clippy -- -D warnings` クリーン / `cargo test` 11 passed（設定 roundtrip・検証・破損ファイル、sidecar 起動停止・グループ kill・早期終了・Node 無し等）
- 手動 E2E（xvfb でリリースバイナリ起動）:
  - 実在パス（データセット直下に monks/ と sources/、書き出し先は別 dir）: `/api/health` が両パス ok、`/api/entities/monks` が両方の内容を 200 で返す。CLI 配置（PHRAPEDIA_ROOT/data）も従来どおり 200
  - 存在しないパス: アプリ・sidecar は起動、health がパスごとのエラーを返し、entities は 409。アプリ終了後に sidecar・ポートとも残存なし

### Linux build
- `npm run tauri:build` 成功: deb (11.6 MiB) / rpm (11.8 MiB) / AppImage (85.5 MiB)。webkit2gtk 等は導入済みで追加 apt なし。
- macOS バイナリは未作成（GitHub Actions 担当）。

### 未解決
- Node は `PATH` 上の `node` 依存（Homebrew 前提）。Node 不在時の挙動は cargo test（`missing_entry_is_an_error_not_a_panic` 等）の範囲のみ検証。
- GUI（WebView 内の操作）の目視確認は未実施。xvfb での起動と API 応答のみ確認。
