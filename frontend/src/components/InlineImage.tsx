import { createReactInlineContentSpec } from '@blocknote/react'

/**
 * 文中に置く画像（試作）。表のセルにはブロックを入れられないので、セルの中の画像はこれで置く。
 * 大きさはセル（列）の幅に収まる。拡大表示はダブルクリック（Editor.tsx の useImageZoom）
 */
export const InlineImage = createReactInlineContentSpec(
  {
    type: 'inlineImage',
    propSchema: { url: { default: '' }, name: { default: '' } },
    content: 'none',
  },
  {
    render: ({ inlineContent }) => {
      const { url, name } = inlineContent.props
      return (
        <span className="inline-image" contentEditable={false}>
          {url ? <img className="inline-image-img" src={url} alt={name} draggable={false} /> : '画像'}
        </span>
      )
    },
  },
)
