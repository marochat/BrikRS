#!/usr/bin/env bash
# 開発機でイメージをビルドし、サーバーに転送して読み込ませる
#   使い方: deploy/push-image.sh <ssh 先>
# イメージには brikrs:<バージョン> と brikrs:latest を付ける（バージョンは backend/Cargo.toml）。
# サーバーの compose は BRIKRS_TAG で使う版を選べる（戻すときは前の版を指定する）
set -euo pipefail
cd "$(dirname "$0")/.."

HOST="${1:?使い方: deploy/push-image.sh <ssh 先>}"
VERSION="$(grep -m1 '^version' backend/Cargo.toml | cut -d'"' -f2)"

# リリースしたものと同じ中身かを確かめる（コミットしていない変更やタグの付け忘れ）
if [[ -n "$(git status --porcelain)" ]]; then
  echo "警告: コミットしていない変更があります。v${VERSION} のタグの中身とイメージが一致しません" >&2
fi
if [[ "$(git describe --tags --exact-match 2>/dev/null || true)" != "v${VERSION}" ]]; then
  echo "警告: 今のコミットに v${VERSION} のタグが付いていません（scripts/bump-version.sh → コミット → git tag）" >&2
fi
read -r -p "brikrs:${VERSION} をビルドして ${HOST} へ転送します。よろしいですか？ [y/N] " ans
[[ "$ans" == [yY] ]] || exit 1

docker build -t "brikrs:${VERSION}" -t brikrs:latest .
echo "==> brikrs:${VERSION} を ${HOST} へ転送します"
docker save "brikrs:${VERSION}" | gzip | ssh "$HOST" 'gunzip | docker load'
ssh "$HOST" "docker tag brikrs:${VERSION} brikrs:latest"
echo "==> 完了。${HOST} で: cd <brikrs のディレクトリ> && docker compose up -d"
echo "    （前の版に戻すとき: BRIKRS_TAG=<前の版> docker compose up -d。DB のマイグレーションを含む版なら DB のバックアップも戻す）"
