import { useState, type FormEvent } from 'react'
import { api, type PageDetail } from '../api'
import { useGroups, type Access } from '../access'
import { AccessPicker } from './AccessPicker'
import { useApp } from '../AppContext'
import Modal from './Modal'

type Props = {
  page: Pick<PageDetail, 'id' | 'is_draft' | 'visibility' | 'group_ids'>
  onSaved: (p: PageDetail) => void
  onClose: () => void
}

/** ページの「…」→「公開範囲…」：下書きと公開範囲（作った人と管理者だけが開ける） */
export default function AccessDialog({ page, onSaved, onClose }: Props) {
  const { me } = useApp()
  const groups = useGroups(me)
  const [draft, setDraft] = useState(page.is_draft)
  const [access, setAccess] = useState<Access>({ visibility: page.visibility, group_ids: page.group_ids })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!me) return null

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const p = await api.updateSettings(page.id, {
        is_draft: draft,
        visibility: access.visibility,
        group_ids: access.visibility === 'groups' ? access.group_ids : [],
      })
      onSaved(p)
      onClose()
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal onClose={onClose}>
      <form onSubmit={submit}>
        <div className="modal-header">
          <h2 className="modal-title fs-5 brand-font">公開範囲</h2>
          <button type="button" className="btn-close" aria-label="閉じる" onClick={onClose} />
        </div>
        <div className="modal-body vstack gap-3">
          <div className="form-check form-switch">
            <input
              className="form-check-input"
              type="checkbox"
              role="switch"
              id="access-draft"
              checked={draft}
              onChange={(e) => setDraft(e.target.checked)}
            />
            <label className="form-check-label" htmlFor="access-draft">
              下書き <span className="muted small">（作った人と管理者だけが見られます）</span>
            </label>
          </div>
          <fieldset className={draft ? 'opacity-75' : undefined}>
            <legend className="fs-6 mb-1">{draft ? '下書きを外したときの公開範囲' : '公開範囲'}</legend>
            <AccessPicker value={access} onChange={setAccess} groups={groups} me={me} idPrefix="access" />
          </fieldset>
          <p className="muted small m-0">
            子ページ（タイムラインの投稿・返信も）にも効きます。親ページの範囲が狭ければ、そちらも満たす人だけが見られます。
          </p>
          {error && <p className="error m-0">{error}</p>}
        </div>
        <div className="modal-footer">
          <button type="button" className="btn btn-outline-secondary" onClick={onClose}>
            キャンセル
          </button>
          <button type="submit" className="btn btn-primary" disabled={busy}>
            保存
          </button>
        </div>
      </form>
    </Modal>
  )
}
