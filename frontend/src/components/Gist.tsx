import { useState } from 'react'
import { createReactBlockSpec } from '@blocknote/react'
import { api } from '../api'
import { gistProps } from '../gist'
import GistView from './GistView'

/**
 * GitHub Gist のブロック。内容は挿入時にサーバーが GitHub の API から取得して props に保存する
 * （表示のたびに外部へ取りに行かない）。編集画面の「↻」で最新の内容を取り直せる。
 * URL の入力は「/」メニューから開くダイアログで行う（GistDialog）
 */
export const Gist = createReactBlockSpec(
  {
    type: 'gist',
    propSchema: {
      gistId: { default: '' },
      url: { default: '' },
      owner: { default: '' },
      description: { default: '' },
      /** ファイルの一覧（GistFile[]）の JSON */
      files: { default: '[]' },
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
          editor.updateBlock(block, { props: gistProps(await api.gist(p.url || p.gistId)) })
        } catch (err) {
          setError((err as Error).message)
        } finally {
          setBusy(false)
        }
      }

      return (
        <div className="gist-block" contentEditable={false}>
          <GistView
            {...p}
            actions={
              editor.isEditable && (
                <button
                  type="button"
                  className="gist-reload"
                  title="GitHub から最新の内容を取り直す"
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
