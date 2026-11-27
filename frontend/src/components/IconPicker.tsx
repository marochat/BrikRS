import { useEffect, useRef, useState } from 'react'

const EMOJIS =
  '📄 📝 📚 📌 📎 💡 ⭐ 🔥 ✅ ❓ ❗ 🎮 🎵 🎨 📷 🎬 🍵 🍣 🍜 ☕ 🐱 🐶 🌸 🌙 ☀️ 🌈 🏠 🏢 🚀 ✈️ 🚃 💻 🛠️ ⚙️ 🔒 🔑 📊 📅 🗂️ 🧪'.split(
    ' ',
  )

type Props = {
  onPick: (icon: string) => void
  onClose: () => void
  hasIcon: boolean
}

export default function IconPicker({ onPick, onClose, hasIcon }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const [custom, setCustom] = useState('')

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose()
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [onClose])

  return (
    <div className="icon-picker card shadow" ref={ref}>
      <div className="icon-grid">
        {EMOJIS.map((e) => (
          <button key={e} onClick={() => onPick(e)}>
            {e}
          </button>
        ))}
      </div>
      <form
        className="icon-custom"
        onSubmit={(e) => {
          e.preventDefault()
          if (custom.trim()) onPick(custom.trim())
        }}
      >
        <input
          className="form-control form-control-sm"
          value={custom}
          onChange={(e) => setCustom(e.target.value)}
          placeholder="絵文字を入力"
          maxLength={8}
        />
        <button type="submit" className="btn btn-sm btn-outline-secondary">
          設定
        </button>
        {hasIcon && (
          <button type="button" className="btn btn-sm btn-outline-danger" onClick={() => onPick('')}>
            削除
          </button>
        )}
      </form>
    </div>
  )
}
