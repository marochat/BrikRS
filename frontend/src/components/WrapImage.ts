import { createImageBlockSpec } from '@blocknote/core'

/*
 * 画像の文字の回り込み（左・右）。
 * BlockNote の画像ブロックに `wrap` の属性を足しただけで、アップロード・キャプション・拡大表示などはそのまま。
 * 属性は既定値（none）以外のとき `data-wrap="left"` のようにブロックの DOM に出るので、見た目は CSS で付ける
 * （index.css の「画像の回り込み」。閲覧画面の BlockView も同じ属性を見る）
 */

export type ImageWrap = 'none' | 'left' | 'right'

const image = createImageBlockSpec()

export const WrapImage = {
  ...image,
  config: {
    ...image.config,
    propSchema: {
      ...image.config.propSchema,
      wrap: { default: 'none' as ImageWrap, values: ['none', 'left', 'right'] as ImageWrap[] },
    },
  },
}
