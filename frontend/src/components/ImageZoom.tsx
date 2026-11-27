import { useEffect } from 'react'
import { createPortal } from 'react-dom'

/** 画像の拡大表示。背景のクリック・Esc で閉じる（画面より小さい画像は元の大きさのまま） */
export default function ImageZoom({ src, alt, onClose }: { src: string; alt: string; onClose: () => void }) {
  useEffect(() => {
    // 編集履歴のモーダルの中でも開くので、Esc はここで止めて外側のモーダルまで閉じないようにする
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.stopPropagation()
      onClose()
    }
    window.addEventListener('keydown', onKey, true)
    const prev = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      window.removeEventListener('keydown', onKey, true)
      document.body.style.overflow = prev
    }
  }, [onClose])

  return createPortal(
    <div className="image-zoom" role="dialog" aria-modal="true" aria-label={alt || '画像'} onClick={onClose}>
      <img src={src} alt={alt} />
    </div>,
    document.body,
  )
}
