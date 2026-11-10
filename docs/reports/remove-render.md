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

## review (review worker)

verdict: ok（修正が必要な指摘なし）。対象: 既定 branch（2476352, v0.2.0）...9aec256。

### 指摘一覧
| # | severity | 内容 | 修正 |
|---|---|---|---|
| 1 | info | `.gitignore` に旧 Web 版由来の `.data` / `.nitro` が残る（`.output` は generate の出力先 `.output-frontend` と別だが nuxt 一般の出力先で無害）。実害なし | 変更なし（指示外・残置の判断は妥当） |
| 2 | info | 終了告知 `Web 版（Render）は 0.2.0 で終了。履歴は tag web-legacy-render` が README に 2 回（冒頭の非互換リスト、開発コマンド節末尾）、CLAUDE.md・ARCHITECTURE・NATIVE_API に各 1 回。重複だが各文書で単独で読まれるため許容 | 変更なし |

### 確認結果
- 削除: `render.yaml`、`scripts/render-start.mjs` が無い。`package.json` scripts は dev / generate / postinstall / typecheck / test / tauri / tauri:dev / tauri:build のみ（build / preview / start / start:render なし）。
- 残存検索: `grep -rniI render`（node_modules / .git / docs/reports / lock 除外、一般語 render() 等を除く）の残りは終了告知 5 件のみ。`npm start|build|preview` / `start:render` / `render-start` / `render.yaml` / `NUXT_DATA_DIR` / `NITRO` の参照なし。`localhost:3000` は README の「HTTP /mcp は廃止」の記述、`PHRAPEDIA_ROOT` は NATIVE_API の「廃止」記述のみ（いずれも史実の説明）。`.env.example` は元から無い。
- 消し過ぎなし: `tauri.conf.json` の `beforeBuildCommand=npm run generate` / `beforeDevCommand=npm run dev` / `frontendDist=../.output-frontend/public` と `nuxt.config.ts`（nitro.output=.output-frontend）は整合。`release.yml` は `npm ci` と tauri-action のみ、`release.sh` は `npm ci` / `npm test` / `npm run typecheck` のみで、削除した scripts を参照しない。`.gitignore` の `.output` は無害、`sources.rs` の `.output` 無視と `lib.rs` テスト文言（`no .output/server resource` の表明）は挙動・検証に関わるため残置が妥当。
- 文書: README の目次は 1〜15 の連番で、本文の h2 と一致（目次に無い h2 は「目次」自身と従来から目次外の「出典」のみ）。README / CLAUDE.md / ARCHITECTURE / NATIVE_API の内部リンク・アンカー・相対ファイルリンクに切れなし（スクリプトで全件検査）。`docs/reports/` の差分は `remove-render.md` の新規追加のみ。impl の指示外 2 件（`NATIVE_API.md` の該当 1 行の終了告知への置換、`app/utils/dialog.ts` のコメントから `Render /` を削除）は、前者は「廃止事項」リスト内の Render 記述の置換で情報損失なし、後者はコメントのみで挙動不変（`git diff` で確認）。いずれも妥当。
- tag: `git ls-remote origin 'refs/tags/web-legacy-render*'` → annotated tag 642f3ae、`^{}` = 5621c6f = `origin/render`（render branch は 5621c6f のまま不変）。message は「旧 Web 版（Nuxt SSR + Render）の最終状態。デスクトップ版 0.2.0 で Render 運用を終了」。
- version: tauri.conf.json / package.json / Cargo.toml / package-lock は 0.2.0 のまま。コードの変更は dialog.ts のコメントのみ。秘密情報なし。3 commit（chore 削除 / docs 告知 / docs report）すべて末尾に Co-Authored-By と Claude-Session。
- テスト: `npm ci` OK / `npm test` 83 passed / `npm run typecheck` exit 0 / `npm run generate` OK（`.output-frontend/public/index.html` 出力）/ `cd src-tauri && cargo check` OK / `release.yml` を yaml で parse（jobs: build, cask）。
