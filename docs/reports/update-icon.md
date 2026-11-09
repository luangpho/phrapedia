# 更新アイコン（起動時の自動ダイアログをやめる）

branch: `feat/update-icon`。起動時に `ask` を自動表示していた更新を、サイドバーの更新アイコンから起動する方式に変えた。手本は bantou desktop のヘッダー更新ボタン（`packages/desktop/src/composables/useUpdater.ts` と `AppHeader.vue`）。

## 設計

- 状態機械（`app/composables/useUpdater.ts`、モジュール単位のシングルトン）: `idle / checking / available(version, notes) / downloading(percent) / installing / error(message) / upToDate`。
- 起動時（`app/plugins/tauri.client.ts` → `init()`）と 6 時間ごとに静かに `check()`。更新があれば `available` になりアイコンにバッジ + 新しい版の tooltip。ダイアログも toast も出さない。静かな check の失敗は `console.info` のみで、既存の `available` は維持。
- アイコン押下（`handleClick`）: `available` なら `confirmDialog`（plugin-dialog `ask`。版・現在の版・release notes を表示）→ `downloadAndInstall`（Progress から %）→ `relaunch`。`available` でなければその場で `check()` し、無ければ「最新です（現在 x.y.z）」toast、あれば確認へ進む。
- 失敗は toast（日本語）。権限エラー（`Permission denied` / `os error 13` / `Operation not permitted` / `Read-only` 等）は `brew upgrade --cask phrapedia` を Homebrew を入れたユーザーで実行する案内に差し替え。
- 多重実行防止: 内部 `busy` フラグ（確認ダイアログ中を含む）と、`checking / downloading / installing` 中はボタン無効 + 静かな check もスキップ。
- 現在の版: `@tauri-apps/api/app` の `getVersion` を tooltip に表示（通常時は「更新を確認（現在 x.y.z）」）。
- UI: `app/components/desktop/UpdateButton.vue`。`app/layouts/default.vue` のサイドバーヘッダーに配置（折りたたみ時は幅が足りないためフッターへ）。`UChip`（バッジ）・`UTooltip`・lucide アイコン、色は Nuxt UI のトークン（primary / neutral / error）のみで、ダークモードでも色指定は崩れない。`aria-label` 付き。`v-if="enabled"` なのでブラウザでは DOM に出ない。
- Tauri plugin はすべて動的 import。非 Tauri では `init()` が最初の `isTauriRuntime()` で戻るため plugin は評価されない。

## capabilities / Rust

- `updater_enabled`（Tauri command）を追加。フロントが updater 有効かを知る手段が既存に無かったため（`plugins.updater` が無い build では `check()` が reject するだけで、有無を判別できない）。`lib.rs` の条件付き plugin 登録と同じ判定を `updater_configured()` に括り出して共有。
- `capabilities/default.json` は変更なし。必要な permission（`updater:default`、`process:allow-restart`、`dialog:allow-message`）は既存。`getVersion` は `core:default` に含まれる。自作 command は build.rs で app manifest を作っていないため追加の permission 不要。
- 条件付き登録は維持。updater 設定なしの build は `updater_enabled` が false を返し、アイコンは出ない。

## テスト結果

- `npm ci && npm test`: 86 passed（`tests/updater.test.ts` 19 件を追加。plugin-updater / plugin-process / plugin-dialog / api/core / api/app をモック）。
  - 更新あり・なし・check 失敗（起動時は静か、クリック時は toast）・download 失敗・権限エラーの brew 案内・多重クリック・downloading 中の再実行無視・進捗 %・「あとで」・6 時間間隔・updater 無効・非 Tauri で何もしない（plugin を import しない）。
  - 起動時に `ask` が呼ばれないことを更新あり / なし / 失敗の各テストで確認。
- `npm run typecheck`: exit 0（`vue-router/volar/sfc-route-blocks` の解決警告は変更前から出ている）。
- `cargo check` / `cargo clippy -- -D warnings`: 通過。`cargo test`: 15 passed。ただし 1 回目に `sidecar::tests::free_port_is_usable` が失敗し、再実行 2 回は通過（既存の flaky。`confirm-dialog.md` の記録と同じ。本変更とは無関係、未調査）。
- `npm run tauri:build`（Linux）: deb / rpm / AppImage 成功。
- `xvfb-run` で updater 設定なしのバイナリを 25 秒起動: panic なし、timeout まで生存（exit 124）。起動後に残プロセスなし。

## 未検証

- macOS 実機での表示・ダイアログ・download→relaunch（Linux では WebView 内の挙動と macOS の入れ替え処理を確認できない）。
- 実際の権限エラー文言。tauri-plugin-updater 2.14.1 のソースでは、macOS で入れ替えが PermissionDenied になると plugin が管理者権限での再試行（osascript）に進み、それも失敗すると `Failed to move the new app into place`（`ErrorKind::PermissionDenied`）を返す。この文言と `Permission denied` 系を正規表現で判定している（実機の文言は未観測。別文言なら `isPermissionError` に追加が必要）。
- updater 有効 build（署名鍵つき）の `updater_enabled = true` 経路と、アイコンの実描画（ダーク / ライト、折りたたみ時のフッター配置）。単体テストは composable のみで、`.vue` のマウントテストは無い。
- 6 時間間隔の長時間稼働。

## 備考

- `docs/reports/dist-like-bantou.md` の旧挙動の記述（起動時 1 回 check → 確認）は当時の記録としてそのまま残した。現行の挙動は README「更新方法」と CLAUDE.md「配布」を参照。
- bantou は確認ダイアログ無しで即 `downloadAndInstall` するが、要件に従い phrapedia は確認ダイアログを挟む。
