#!/usr/bin/env bash
# phrapedia の release を 1 コマンドで行う（bash 3.2 で動く。macOS の利用者端末で実行する）。
# Usage: scripts/release.sh [--dry-run] <version>
#   version   : 例 0.1.1（先頭の v は付けない）。既定 branch の今の版と違うこと
#   --dry-run : 検査・clone・npm ci・test・typecheck・版上げ commit まで。push も tag も release 待ちもしない
# 流れ: 既定 branch を一時 dir に新規 clone → npm ci・npm test・typecheck → 版を揃えて 1 commit
#       → 既定 branch を push → tag v<version> を push → release workflow の完了を待つ → 後始末
# ビルド・署名・GitHub Release・cask 更新は GitHub Actions（.github/workflows/release.yml）が行う。ここでは tauri build しない。
# 前提: git・node・npm・gh（gh auth login 済み）。
# commit の末尾に足す行があれば ~/.config/bantou/release-trailer に置く（無ければ足さない）。
# 環境変数: PHRAPEDIA_REPO_URL で clone 元を差し替えられる（検証用。既定は GitHub の luangpho/phrapedia）。
set -euo pipefail

usage() {
  echo "Usage: $0 [--dry-run] <version>" >&2
  exit 1
}

DRY_RUN=0
while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    *) usage ;;
  esac
  shift
done

VERSION="${1:-}"
[[ -z "$VERSION" ]] && usage
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "error: version must look like 0.1.1: $VERSION" >&2; exit 1; }

SLUG="luangpho/phrapedia"
TAP_SLUG="luangpho/homebrew-tap"
REPO_URL="${PHRAPEDIA_REPO_URL:-https://github.com/$SLUG.git}"
TAG="v$VERSION"
TRAILER_FILE="$HOME/.config/bantou/release-trailer"
WORK=""

step() { echo "== $(date '+%H:%M:%S') $*"; }

# push 前に失敗したら clone を消す（再実行できるように）。push 後の失敗では残し、続きを手で行う
PUSHED=0
on_exit() {
  local rc=$?
  if [[ -n "$WORK" && -d "$WORK" ]]; then
    if [[ $rc -ne 0 && "$PUSHED" == 1 ]]; then
      echo "error: failed after push; the clone is kept at $WORK" >&2
    else
      cd /tmp
      rm -rf "$WORK"
    fi
  fi
}
trap on_exit EXIT

command -v node >/dev/null || { echo "error: node not found" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "error: gh is not logged in to github.com" >&2; exit 1; }
if gh release view "$TAG" -R "$SLUG" >/dev/null 2>&1; then
  echo "error: release $TAG already exists" >&2
  exit 1
fi

commit() {
  local msg="$1"; shift
  if [[ -s "$TRAILER_FILE" ]]; then
    git commit -q -m "$msg" -m "$(cat "$TRAILER_FILE")" "$@"
  else
    git commit -q -m "$msg" "$@"
  fi
}

WORK="$(mktemp -d "${TMPDIR:-/tmp}/phrapedia-rel-XXXXXX")"
step "clone default branch"
git clone -q "$REPO_URL" "$WORK/repo"
cd "$WORK/repo"
BRANCH="$(git symbolic-ref --short HEAD)"
if git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1; then
  echo "error: tag $TAG already exists on origin" >&2
  exit 1
fi
git config user.name >/dev/null 2>&1 || git config user.name "phrapedia release"
git config user.email >/dev/null 2>&1 || git config user.email "release@localhost"
OLD="$(node -p "require('./package.json').version")"
[[ "$OLD" != "$VERSION" ]] || { echo "error: $BRANCH is already $VERSION" >&2; exit 1; }
echo "$BRANCH $(git rev-parse --short HEAD), version $OLD -> $VERSION"

step "install and test"
npm ci
npm test
npm run typecheck
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || { echo "error: npm ci/test changed tracked files:" >&2; git status --short --untracked-files=no >&2; exit 1; }

step "bump $OLD -> $VERSION"
# node で書き換える（sed -i は GNU/BSD で書式が違うため）。各ファイルの「この版」を 1 箇所ずつ置換する
OLD_VERSION="$OLD" NEW_VERSION="$VERSION" node -e '
const fs = require("fs");
const oldV = process.env.OLD_VERSION, newV = process.env.NEW_VERSION;
const edit = (file, fn) => {
  const src = fs.readFileSync(file, "utf8");
  const out = fn(src);
  if (out === src) { console.error("error: no change in " + file); process.exit(1); }
  fs.writeFileSync(file, out);
};
const q = (s) => s.replace(/\./g, "\\.");
const jsonVersion = new RegExp("(\"version\": \")" + q(oldV) + "(\")", "g");
// package.json: トップレベルの version のみ（最初の 1 件）
edit("package.json", (s) => s.replace(new RegExp("(\"version\": \")" + q(oldV) + "(\")"), "$1" + newV + "$2"));
// package-lock.json: ルートと packages[""] の 2 件（先頭から 2 件だけ）
let n = 0;
edit("package-lock.json", (s) => s.replace(jsonVersion, (m, a, b) => (++n <= 2 ? a + newV + b : m)));
edit("src-tauri/tauri.conf.json", (s) => s.replace(new RegExp("(\"version\": \")" + q(oldV) + "(\")"), "$1" + newV + "$2"));
// Cargo.toml: [package] の version（行頭、最初の 1 件）
edit("src-tauri/Cargo.toml", (s) => s.replace(new RegExp("^version = \"" + q(oldV) + "\"$", "m"), "version = \"" + newV + "\""));
// Cargo.lock: name = "phrapedia" の直後の version だけ（同じ版の別 crate があり得る）
edit("src-tauri/Cargo.lock", (s) => s.replace(new RegExp("(name = \"phrapedia\"\\nversion = \")" + q(oldV) + "(\")"), "$1" + newV + "$2"));
'
for spec in package.json:1 package-lock.json:2 src-tauri/tauri.conf.json:1 src-tauri/Cargo.toml:1 src-tauri/Cargo.lock:1; do
  f="${spec%%:*}"
  want="${spec##*:}"
  n="$(git diff -U0 -- "$f" | grep -c '^+[^+]' || true)"
  [[ "$n" == "$want" ]] || { echo "error: $f changed $n lines (expected $want)" >&2; exit 1; }
done
git diff -U0 | grep '^[+-][^+-]' | grep -v "$VERSION" | grep -v "$OLD" && { echo "error: unexpected lines changed" >&2; exit 1; } || true
commit "phrapedia $VERSION" -a
SHA="$(git rev-parse HEAD)"

if [[ "$DRY_RUN" == 1 ]]; then
  step "dry-run: check push access"
  git push --dry-run origin "$BRANCH"
  git push --dry-run origin "$SHA:refs/tags/$TAG"
  git log -1 --stat --format='%h %s'
  echo "dry-run ok: $VERSION (sha $SHA); nothing was pushed or tagged"
  exit 0
fi

PUSHED=1
step "push $BRANCH"
git push origin "$BRANCH"
step "push tag $TAG"
git tag "$TAG" "$SHA"
git push origin "$TAG"

step "wait for the release workflow"
RUN_ID=""
for _ in $(seq 1 30); do
  RUN_ID="$(gh run list -R "$SLUG" --workflow release.yml --event push --branch "$TAG" --json databaseId,headSha -q "[.[] | select(.headSha == \"$SHA\")][0].databaseId // empty" 2>/dev/null || true)"
  [[ -n "$RUN_ID" ]] && break
  sleep 5
done
[[ -n "$RUN_ID" ]] || { echo "error: the release workflow run for $TAG did not appear" >&2; exit 1; }
gh run watch "$RUN_ID" -R "$SLUG" --exit-status

step "result"
gh release view "$TAG" -R "$SLUG"
echo "-- tap Casks/phrapedia.rb"
gh api -H "Accept: application/vnd.github.raw" "repos/$TAP_SLUG/contents/Casks/phrapedia.rb" | grep -E '^ *version '

PUSHED=0
echo "released $VERSION (sha $SHA)"
