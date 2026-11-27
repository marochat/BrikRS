# syntax=docker/dockerfile:1
# BrikRS：フロントのビルド → Rust のビルド → distroless に実行ファイルと静的ファイルだけを置く
# ビルドは開発機で行い、サーバーにはイメージだけを持ち込む（サーバー上でビルドしない）

FROM node:24-slim AS frontend
WORKDIR /src
COPY frontend/package.json frontend/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY frontend/ ./
# バージョンの正本（package.json と食い違っていればビルドが止まる）
COPY backend/Cargo.toml /backend/Cargo.toml
RUN npm run build

FROM rust:1.99-slim-trixie AS backend
WORKDIR /src
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
# 初期データ（サンプルページ）の画像。実行ファイルに埋め込む
COPY backend/seed ./seed
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked \
 && cp target/release/brikrs /brikrs
# 実行ユーザー(nonroot=65532)が書き込めるデータ置き場を用意しておく
RUN mkdir -p /data/uploads

FROM gcr.io/distroless/cc-debian13:nonroot
COPY --from=backend /brikrs /app/brikrs
COPY --from=frontend /src/dist /app/static
# ライセンスと、同梱しているもののライセンス表記
COPY LICENSE NOTICE /app/
COPY --from=backend --chown=65532:65532 /data /data
ENV BIND_ADDR=0.0.0.0:8080 \
    STATIC_DIR=/app/static \
    UPLOAD_DIR=/data/uploads \
    BRANDING_DIR=/data/branding \
    RUST_LOG=info,sea_orm=warn,sqlx=warn
VOLUME ["/data"]
EXPOSE 8080
USER nonroot
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s CMD ["/app/brikrs", "healthcheck"]
ENTRYPOINT ["/app/brikrs"]
CMD ["serve"]
