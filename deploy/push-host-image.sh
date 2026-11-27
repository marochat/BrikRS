#!/usr/bin/env bash
# 開発機（ホストの CPU アーキテクチャが amd64 でなくてもよい）でイメージをビルドし、サーバーに転送して読み込ませる
#   使い方: deploy/push-host-image.sh <ssh 先>
# push-image.sh との違い：buildx で linux/amd64 を明示してビルドする（Mac(arm64) 等からでもサーバー(amd64)向けに作れる。
#   ホストが amd64 でなければ QEMU エミュレーションを使うため、push-image.sh よりビルドが遅い）
# ローカルのタグは brikrs-amd64:<バージョン>（docker compose up -d --build で作るローカル確認用の
#   brikrs:latest ＝ホストのネイティブ architecture とは別名にして取り違えを防ぐ）。
# サーバー側では brikrs:<バージョン> / brikrs:latest に付け直す（サーバーの compose が参照する名前はそのまま）
set -euo pipefail
cd "$(dirname "$0")/.."

HOST="${1:?使い方: deploy/push-host-image.sh <ssh 先>}"
VERSION="$(grep -m1 '^version' backend/Cargo.toml | cut -d'"' -f2)"

# リリースしたものと同じ中身かを確かめる（コミットしていない変更やタグの付け忘れ）
if [[ -n "$(git status --porcelain)" ]]; then
  echo "警告: コミットしていない変更があります。v${VERSION} のタグの中身とイメージが一致しません" >&2
fi
if [[ "$(git describe --tags --exact-match 2>/dev/null || true)" != "v${VERSION}" ]]; then
  echo "警告: 今のコミットに v${VERSION} のタグが付いていません（scripts/bump-version.sh → コミット → git tag）" >&2
fi
read -r -p "brikrs-amd64:${VERSION}（linux/amd64）をビルドして ${HOST} へ転送します。よろしいですか？ [y/N] " ans
[[ "$ans" == [yY] ]] || exit 1

docker buildx build --platform linux/amd64 --load -t "brikrs-amd64:${VERSION}" .
echo "==> brikrs-amd64:${VERSION} を ${HOST} へ転送します"
docker save "brikrs-amd64:${VERSION}" | gzip | ssh "$HOST" 'gunzip | docker load'
ssh "$HOST" "docker tag brikrs-amd64:${VERSION} brikrs:${VERSION} && docker tag brikrs-amd64:${VERSION} brikrs:latest"
echo "==> 完了。${HOST} で: cd <brikrs のディレクトリ> && docker compose up -d"
echo "    （前の版に戻すとき: BRIKRS_TAG=<前の版> docker compose up -d。DB のマイグレーションを含む版なら DB のバックアップも戻す）"
