import { useState } from 'react'
import { api, type PageDetail } from '../api'
import { displayTitle, useApp } from '../AppContext'
import Modal from './Modal'

type Props = {
  onPick: (tpl: PageDetail) => void
  onClose: () => void
}

/** テンプレート（「テンプレートにする」を付けたページ）から選んで、その本文で子ページを作る */
export default function TemplatePicker({ onPick, onClose }: Props) {
  const { pages } = useApp()
  const templates = pages.filter((p) => p.is_template)
  const [busy, setBusy] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  const pick = async (id: number) => {
    setBusy(id)
    setError(null)
    try {
      onPick(await api.getPage(id))
    } catch (e) {
      setError((e as Error).message)
      setBusy(null)
    }
  }

  return (
    <Modal onClose={onClose} dialogClassName="modal-dialog-centered">
      <div className="modal-header">
        <h5 className="modal-title">テンプレートから作成</h5>
        <button type="button" className="btn-close" onClick={onClose} aria-label="閉じる" />
      </div>
      <div className="modal-body">
        {error && <p className="error">{error}</p>}
        {templates.length === 0 ? (
          <p className="muted">
            まだテンプレートがありません。ページの「…」メニューから「テンプレートにする」を選ぶと、ここに出てきます。
          </p>
        ) : (
          <ul className="list-group">
            {templates.map((t) => (
              <li key={t.id} className="list-group-item p-0">
                <button
                  type="button"
                  className="dropdown-item d-flex align-items-center gap-2 py-2"
                  disabled={busy !== null}
                  onClick={() => pick(t.id)}
                >
                  <span className="tree-icon">{t.icon || '📄'}</span>
                  <span>{displayTitle(t.title)}</span>
                  {busy === t.id && <span className="muted small ms-auto">作成中…</span>}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </Modal>
  )
}
