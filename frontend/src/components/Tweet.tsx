import { useState } from 'react'
import { createReactBlockSpec } from '@blocknote/react'
import { api } from '../api'
import { statusUrl, tweetProps } from '../tweet'
import TweetView from './TweetView'

/**
 * X の投稿のブロック。投稿者・本文・日付は挿入時にサーバーが X の oEmbed から取得して props に保存する
 * （表示のたびに外部へ取りに行かない）。編集画面の「↻」で取り直せる（page post で ID だけ取り込んだものも）。
 * URL の入力は「/」メニューから開くダイアログで行う（TweetDialog）
 */
export const Tweet = createReactBlockSpec(
  {
    type: 'tweet',
    propSchema: {
      tweetId: { default: '' },
      tweetHandle: { default: '' },
      author: { default: '' },
      body: { default: '' },
      date: { default: '' },
      media: { default: false },
    },
    content: 'none',
  },
  {
    render: function Render({ block, editor }) {
      const [busy, setBusy] = useState(false)
      const [error, setError] = useState<string | null>(null)
      const p = block.props

      const reload = async () => {
        setBusy(true)
        setError(null)
        try {
          editor.updateBlock(block, { props: tweetProps(await api.tweet(statusUrl(p.tweetId, p.tweetHandle))) })
        } catch (err) {
          setError((err as Error).message)
        } finally {
          setBusy(false)
        }
      }

      return (
        <div className="tweet-block" contentEditable={false}>
          <TweetView
            {...p}
            actions={
              editor.isEditable &&
              p.tweetId && (
                <button
                  type="button"
                  className="tweet-reload"
                  title="X から投稿の内容を取り直す"
                  disabled={busy}
                  // エディタに選択を取られないように
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={reload}
                >
                  {busy ? '…' : '↻'}
                </button>
              )
            }
          />
          {error && <p className="error small m-0 mt-1">{error}</p>}
        </div>
      )
    },
  },
)
