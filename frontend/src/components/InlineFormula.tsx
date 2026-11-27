import { useState } from 'react'
import { createReactInlineContentSpec } from '@blocknote/react'
import { Katex } from '../katex'
import FormulaDialog from './FormulaDialog'

/**
 * 文章中の数式：LaTeX を KaTeX でインライン表示する。content: 'none' で、LaTeX は props に持つ。
 * クリックするとダイアログで編集（挿入直後は空なので自動的に開く）
 */
export const InlineFormula = createReactInlineContentSpec(
  {
    type: 'inlineFormula',
    propSchema: { formula: { default: '' } },
    content: 'none',
  },
  {
    render: function Render({ inlineContent, updateInlineContent, editor }) {
      const { formula } = inlineContent.props
      const [open, setOpen] = useState(!formula && editor.isEditable)

      // ダイアログを閉じるとモーダルにフォーカスが残り、続けて打鍵できなくなるのでエディタへ戻す
      const close = () => {
        setOpen(false)
        editor.focus()
      }
      const done = (f: string) => {
        updateInlineContent({ type: 'inlineFormula', props: { formula: f } })
        close()
      }

      return (
        <span
          className={`formula-inline${formula ? '' : ' is-empty'}`}
          contentEditable={false}
          onClick={() => editor.isEditable && setOpen(true)}
        >
          {formula ? <Katex formula={formula} display={false} /> : '数式'}
          {open && <FormulaDialog initial={formula} display={false} onDone={done} onClose={close} />}
        </span>
      )
    },
  },
)
