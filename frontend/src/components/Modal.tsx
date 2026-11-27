import { useEffect, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

type Props = {
  onClose: () => void
  /** modal-content に付けるクラス */
  className?: string
  /** modal-dialog に付けるクラス（modal-lg、modal-fullscreen-md-down など） */
  dialogClassName?: string
  children: ReactNode
}

/** Bootstrap のモーダルの見た目だけを使う（開閉は React で行い、Bootstrap の JS は読み込まない） */
export default function Modal({ onClose, className = '', dialogClassName = '', children }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    document.addEventListener('keydown', onKey)
    // 背後のページがスクロールしないようにする
    const prev = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', onKey)
      document.body.style.overflow = prev
    }
  }, [onClose])

  return (
    <>
      <div
        className="modal d-block"
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        onMouseDown={(e) => e.target === e.currentTarget && onClose()}
      >
        <div className={`modal-dialog ${dialogClassName}`}>
          <div className={`modal-content ${className}`}>{children}</div>
        </div>
      </div>
      <div className="modal-backdrop show" />
    </>
  )
}

/**
 * エディタの中（ブロックやそのボタン）から開くダイアログ用の Modal。body に出し、キー操作とクリックを外へ伝えない。
 * createPortal で DOM 上は document.body 直下でも、React のイベントは元のツリー（ブロック）を伝って上まで届く
 * （ポータルは DOM の配置先を変えるだけ）。キーを止めないと Enter や / がエディタや投稿欄（Ctrl+Enter）に効き、
 * クリックを止めないと「クリックで開く」ブロックで、閉じるボタンのクリックがブロックまで伝わって開き直してしまう
 */
export function EditorModal(props: Props) {
  return createPortal(
    <div onKeyDown={(e) => e.stopPropagation()} onClick={(e) => e.stopPropagation()}>
      <Modal {...props} />
    </div>,
    document.body,
  )
}
