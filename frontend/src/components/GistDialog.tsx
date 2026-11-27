import { useState, type FormEvent } from 'react'
import { api, type GistData } from '../api'
import Modal from './Modal'

/** Gist の URL を入力するダイアログ。取得できたら onDone に内容を渡す */
export default function GistDialog({ onDone, onClose }: { onDone: (g: GistData) => void; onClose: () => void }) {
  const [url, setUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (busy || !url.trim()) return
    setBusy(true)
    setError(null)
    try {
      onDone(await api.gist(url.trim()))
    } catch (err) {
      setError((err as Error).message)
      setBusy(false)
    }
  }

  return (
    <Modal onClose={onClose}>
      <form onSubmit={submit}>
        <div className="modal-header">
          <h2 className="modal-title fs-5 brand-font">GitHub Gist</h2>
          <button type="button" className="btn-close" aria-label="閉じる" onClick={onClose} />
        </div>
        <div className="modal-body vstack gap-2">
          <input
            className="form-control"
            type="url"
            inputMode="url"
            placeholder="https://gist.github.com/..."
            value={url}
            autoFocus
            disabled={busy}
            aria-label="Gist の URL"
            onChange={(e) => setUrl(e.target.value)}
          />
          <p className="muted small m-0">
            Gist のファイルを読み込んで埋め込みます（内容はこのページに保存され、後から ↻ で取り直せます）。
          </p>
          {error && <p className="error m-0">{error}</p>}
        </div>
        <div className="modal-footer">
          <button type="button" className="btn btn-outline-secondary" onClick={onClose}>
            キャンセル
          </button>
          <button type="submit" className="btn btn-primary" disabled={busy || !url.trim()}>
            {busy ? '読み込み中…' : '埋め込む'}
          </button>
        </div>
      </form>
    </Modal>
  )
}
