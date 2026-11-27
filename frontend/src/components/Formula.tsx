import { useState } from 'react'
import { createReactBlockSpec } from '@blocknote/react'
import { Katex } from '../katex'
import FormulaDialog from './FormulaDialog'

/**
 * 数式ブロック：独立した行に LaTeX を KaTeX で表示する。本文は持たず（content: 'none'）、
 * LaTeX は props に持つ。クリックするとダイアログで編集（挿入直後は空なので自動的に開く）
 */
export const Formula = createReactBlockSpec(
  {
    type: 'formula',
    propSchema: { formula: { default: '' } },
    content: 'none',
  },
  {
    render: function Render({ block, editor }) {
      const { formula } = block.props
      const [open, setOpen] = useState(!formula && editor.isEditable)

      // ダイアログを閉じるとモーダルにフォーカスが残り、続けて打鍵できなくなるのでエディタへ戻す
      const close = () => {
        setOpen(false)
        editor.focus()
      }
      const done = (f: string) => {
        editor.updateBlock(block, { props: { formula: f } })
        close()
      }

      return (
        <div
          className={`formula-block${formula ? '' : ' is-empty'}`}
          contentEditable={false}
          onClick={() => editor.isEditable && setOpen(true)}
        >
          {formula ? <Katex formula={formula} display /> : <span className="muted">クリックして数式を入力</span>}
          {open && <FormulaDialog initial={formula} display onDone={done} onClose={close} />}
        </div>
      )
    },
  },
)
