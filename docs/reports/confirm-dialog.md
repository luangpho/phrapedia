# 確認ダイアログの window.confirm 置換

branch: `fix/confirm-dialog`。Tauri（wry）の macOS WKWebView は JS の confirm パネルが未実装で常にキャンセル扱いになる恐れがある（PR #3 レビューの info 指摘）。アプリ内の確認を共通関数に寄せた。

## 作業内容

- 洗い出し: `app/` `shared/` で `window.confirm` は 2 箇所（`app/utils/unsaved.ts` のルートガード、`app/pages/[kind]/[slug].vue` の編集取消）。`alert` / `prompt` は 0 件。
- `app/utils/dialog.ts` に `confirmDialog(message, {title, okLabel, cancelLabel}): Promise<boolean>`。`isTauriRuntime()`（`__TAURI_INTERNALS__` 判定）が true なら `@tauri-apps/plugin-dialog` の `ask`（動的 import）、それ以外は `window.confirm`。
- `unsaved.ts` のガードを async 化（`onBeforeRouteLeave` / `onBeforeRouteUpdate`）。`beforeunload` は変更なし。
- `[slug].vue` の `cancelEdit` を async 化。
- vue-router 4.6.4 の `guardToPromiseFn` は guard の戻り値を `Promise.resolve(...).then(next)` で処理する（3 引数未満の guard）。Promise<false> は従来の false と同じく遷移中止になる（ソースで確認）。
- `tests/dialog.test.ts`: Tauri 判定 true で `ask` 呼び出し・結果返却、false で `window.confirm` を確認（plugin-dialog と `isTauriRuntime` はモック）。
- capabilities は変更なし。`@tauri-apps/plugin-dialog` 2.8.4 では `dialog:allow-ask` は deprecated で `allow-message` の別名（ask は message コマンドを呼ぶ）。PR #3 で追加済みの `dialog:allow-message` で足りる。

## 確認結果

- `npm ci && npm test`: 66 passed。`npm run typecheck`: exit 0（`vue-router/volar/sfc-route-blocks` の解決警告は変更前から出ている）。
- `cargo check` / `cargo clippy -- -D warnings` / `cargo test`: 通過。
- Linux で `npm run tauri:build`: deb / rpm / AppImage 生成。
- `xvfb-run` で起動し 25 秒生存（クラッシュなし。設定なし）。
- grep: `app/` `shared/` に `window.confirm` / `confirm(` / `alert(` / `prompt(` は `dialog.ts` 内のブラウザ fallback のみ。

## 残課題

- macOS 実機での ask ダイアログ表示（Linux では確認不可）。
- ブラウザ版の窓内遷移ガードは従来どおり window.confirm（仕様）。

## レビュー（dialog-review）

- 確認 OK: window.confirm / alert / prompt の取りこぼしなし（テンプレート内 @click も無し。削除・破棄は UModal 経由）。Tauri 判定は useUpdater と同じ `isTauriRuntime()`。ask 引数は plugin-dialog 2.8.4 の API と一致（ask は message コマンド経由、`dialog:allow-ask` は deprecated alias で `allow-message` と同義、crate の reference.md で確認）。capabilities は最小のまま。
- 指摘・修正: `ask` が reject（権限不足など）すると cancelEdit / ルートガードで未処理例外になるため、`confirmDialog` で catch して false（編集を破棄しない）を返すよう修正。テスト追加。
- 未対応の軽微事項: 確認中に取消ボタンを連打するとダイアログが重なり得る（状態破壊はなし）。
- 環境メモ: `cargo test` が 1 回だけ 1 件失敗し再実行で 15 件通過（flaky の可能性、未調査）。
