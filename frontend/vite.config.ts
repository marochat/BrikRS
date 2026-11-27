import { readFileSync } from 'node:fs'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// システム全体のバージョンは backend/Cargo.toml が正本。package.json と食い違っていたらビルドを止める
// （scripts/bump-version.sh で両方を一度に上げる）
const appVersion: string = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8')).version
const cargoToml = readFileSync(new URL('../backend/Cargo.toml', import.meta.url), 'utf8')
const cargoVersion = cargoToml.match(/^\[package\][^[]*?^version\s*=\s*"([^"]+)"/m)?.[1]
if (cargoVersion !== appVersion) {
  throw new Error(`バージョンが食い違っています：frontend/package.json=${appVersion}, backend/Cargo.toml=${cargoVersion}`)
}

const backend = process.env.BRIKRS_BACKEND || 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  css: {
    preprocessorOptions: {
      // Bootstrap 5.3 の SCSS は新しい Sass で非推奨の書き方（@import など）を使っているので警告を抑える
      scss: { quietDeps: true, silenceDeprecations: ['import', 'global-builtin', 'color-functions', 'if-function'] },
    },
  },
  server: {
    // 開発時は、Rust サーバーが受け持つパス（API・アップロードした画像・フィード）を中継する
    // 中継先は BRIKRS_BACKEND で変えられる（例：Docker のコンテナなら BRIKRS_BACKEND=http://127.0.0.1:8081 npm run dev）
    proxy: {
      '/api': backend,
      '/uploads': backend,
      '^/(p/\\d+/)?feed\\.atom$': backend,
    },
  },
})
