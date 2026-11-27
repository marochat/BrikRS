import { useState } from 'react'
import { createReactBlockSpec } from '@blocknote/react'
import { linkHost } from '../linkHost'
import LinkCardDialog, { type ImagePos } from './LinkCardDialog'

/**
 * リンクカード：画像と、自分で書いた文章（タイトル＋子ブロック）が一体になった、Web リンクのカード。
 * タイトルはこのブロックの文字、本文は子ブロック（Tab で中に入れ、Shift+Tab で外に出す）。
 * 枠と画像の配置は CSS で、このブロック（.bn-block）と子ブロックをまとめて囲んで作る（index.css）
 */
export const LinkCard = createReactBlockSpec(
  {
    type: 'linkCard',
    propSchema: {
      url: { default: '' },
      image: { default: '' },
      imagePos: { default: 'left' as ImagePos, values: ['left', 'right', 'top'] as const },
    },
    content: 'inline',
  },
  {
    render: function Render({ block, editor, contentRef }) {
      const [open, setOpen] = useState(false)
      const { url, image, imagePos } = block.props
      const pos = image ? imagePos : 'none'
      return (
        <div className={`link-card-head pos-${pos}`}>
          {image && (
            <div className="link-card-image" contentEditable={false}>
              <img src={image} alt="" />
            </div>
          )}
          <div className="link-card-title" ref={contentRef} />
          <div className="link-card-foot" contentEditable={false}>
            {editor.isEditable && (
              <button
                type="button"
                className="link-card-settings"
                title="リンク先・画像の設定"
                // エディタに選択を取られないように
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => setOpen(true)}
              >
                ⚙
              </button>
            )}
            <a className="link-card-host" href={url} target="_blank" rel="noopener noreferrer">
              {url ? linkHost(url) : 'リンク先が未設定'} ↗
            </a>
          </div>
          {open && (
            <LinkCardDialog
              mode="edit"
              initial={{ url, image, imagePos, title: '', description: '' }}
              onClose={() => setOpen(false)}
              onDone={(f) => {
                setOpen(false)
                editor.updateBlock(block, { props: { url: f.url, image: f.image, imagePos: f.imagePos } })
              }}
            />
          )}
        </div>
      )
    },
  },
)
