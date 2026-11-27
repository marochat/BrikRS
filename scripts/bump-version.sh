#!/usr/bin/env bash
# システム全体のバージョンを上げる（backend/Cargo.toml が正本、frontend/package.json も揃える）
#   使い方: scripts/bump-version.sh 0.2.0
# 番号の付け方（0.x 系）：機能追加・DB のマイグレーションを含む → 0.x.0、不具合の修正だけ → 0.x.y
set -euo pipefail
cd "$(dirname "$0")/.."

NEW="${1:-}"
if [[ ! "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "使い方: $0 <x.y.z>（今は $(grep -m1 '^version' backend/Cargo.toml | cut -d'"' -f2)）" >&2
  exit 1
fi

# [package] の version だけを書き換える（依存クレートの version には触れない）
sed -i "0,/^version = \".*\"/s//version = \"$NEW\"/" backend/Cargo.toml
(cd backend && cargo update -p brikrs --offline >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null)
(cd frontend && npm version "$NEW" --no-git-tag-version --allow-same-version >/dev/null)

echo "==> $NEW に更新しました（backend/Cargo.toml, Cargo.lock, frontend/package.json, package-lock.json）"
echo "    次に：CHANGELOG.md に $NEW の変更点を書く → コミット → git tag -a v$NEW -m \"BrikRS $NEW\""
