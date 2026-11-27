import { createReactBlockSpec } from '@blocknote/react'
import BookmarkCard from './BookmarkCard'

/**
 * Web ブックマーク（URL のタイトル・説明・画像のカード）のブロック。
 * 内容は作成時にサーバーで OGP から取得して props に保存する（表示のたびに外部へ取りに行かない）。
 * URL の入力は「/」メニューから開くダイアログで行う（BookmarkDialog）
 */
export const Bookmark = createReactBlockSpec(
  {
    type: 'bookmark',
    propSchema: {
      url: { default: '' },
      title: { default: '' },
      description: { default: '' },
      siteName: { default: '' },
      image: { default: '' },
    },
    content: 'none',
  },
  {
    render: ({ block }) => (
      <div className="bookmark-block" contentEditable={false}>
        <BookmarkCard b={block.props} link={false} />
      </div>
    ),
  },
)
