# Render 残置物の除去

## 変更点
- tag `web-legacy-render`（annotated、render branch 先頭 5621c6f）を push。render branch は未変更。
- 削除: `render.yaml`、`scripts/render-start.mjs`。
- `package.json` scripts から `build` / `preview` / `start` / `start:render` を除去（version は 0.2.0 のまま）。
- 文書: README（節・目次・2 箇所）、CLAUDE.md、docs/ARCHITECTURE.md、docs/NATIVE_API.md を「Web 版（Render）は 0.2.0 で終了。履歴は tag `web-legacy-render`」に置換。
- `app/utils/dialog.ts` のコメント 1 語（`Render /`）のみ削除（挙動変更なし）。

## 確認結果
- `npm ci` / `npm test`（6 files, 83 tests pass）/ `npm run typecheck` / `npm run generate`（`.output-frontend/public` に出力）/ `cd src-tauri && cargo check`: すべて成功。
- `src-tauri/tauri.conf.json` の `beforeBuildCommand` は `npm run generate`、`beforeDevCommand` は `npm run dev`。削除した scripts は参照していない。
- `.github/workflows/*`、`scripts/release.sh`、`src-tauri/*.json` に `build` / `preview` / `start` / `start:render` / `render-start` / `render.yaml` の参照なし。
- `grep -rni render`（node_modules / .git / docs/reports / package-lock.json 除外）の残りは終了告知のみ（rendering 等の一般語を除く）。

## 残した物と理由
- `.gitignore` の `.output`: nuxt 一般の出力先で Tauri ビルドに無害のため。
- `src-tauri/src/core/sources.rs` の `.output` 無視: 挙動に関わるため変更せず。
- `src-tauri/src/lib.rs` のテスト文言 `.output/server`: テスト内の表明メッセージ。挙動に関わるため変更せず。
- `docs/ARCHITECTURE.md` の `.output/server` 記述: 「同梱されない」という現行の事実の説明。
- `docs/reports/*.md`: 過去の記録。
