import { useState, type FormEvent } from 'react'
import { Katex } from '../katex'
import { EditorModal } from './Modal'

type Props = {
  initial: string
  /** ブロック（大きい表示）かインライン（文章の中）か。プレビューの見た目だけに使う */
  display: boolean
  onDone: (formula: string) => void
  onClose: () => void
}

/** 数式（ブロック・インラインの両方）の LaTeX を書くダイアログ。プレビューを見ながら書ける */
export default function FormulaDialog({ initial, display, onDone, onClose }: Props) {
  const [text, setText] = useState(initial)

  const submit = (e: FormEvent) => {
    e.preventDefault()
    const f = text.trim()
    if (!f) return
    onDone(f)
  }

  return (
    <EditorModal onClose={onClose}>
      <form onSubmit={submit}>
        <div className="modal-header">
          <h2 className="modal-title fs-5 brand-font">数式</h2>
          <button type="button" className="btn-close" aria-label="閉じる" onClick={onClose} />
        </div>
        <div className="modal-body vstack gap-3">
          <label className="form-label mb-0">
            LaTeX
            <textarea
              className="form-control mt-1 formula-dialog-input"
              rows={3}
              autoFocus
              placeholder="例：x^2 + y^2 = z^2"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </label>
          {text.trim() && (
            <div className="formula-dialog-preview">
              <Katex formula={text} display={display} />
            </div>
          )}
        </div>
        <div className="modal-footer">
          <button type="button" className="btn btn-outline-secondary" onClick={onClose}>
            キャンセル
          </button>
          <button type="submit" className="btn btn-primary" disabled={!text.trim()}>
            決定
          </button>
        </div>
      </form>
    </EditorModal>
  )
}
