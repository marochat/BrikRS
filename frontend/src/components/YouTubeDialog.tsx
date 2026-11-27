import { useState, type FormEvent } from 'react'
import { parseVideoId, thumbnailUrl } from '../youtube'
import Modal from './Modal'

/** YouTube 動画の URL を入力するダイアログ。動画 ID が読み取れたら onDone に渡す */
export default function YouTubeDialog({ onDone, onClose }: { onDone: (videoId: string) => void; onClose: () => void }) {
  const [url, setUrl] = useState('')
  const id = parseVideoId(url)

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (id) onDone(id)
  }

  return (
    <Modal onClose={onClose}>
      <form onSubmit={submit}>
        <div className="modal-header">
          <h2 className="modal-title fs-5 brand-font">YouTube</h2>
          <button type="button" className="btn-close" aria-label="閉じる" onClick={onClose} />
        </div>
        <div className="modal-body vstack gap-2">
          <input
            className="form-control"
            type="url"
            inputMode="url"
            placeholder="https://www.youtube.com/watch?v=..."
            value={url}
            autoFocus
            aria-label="動画の URL"
            onChange={(e) => setUrl(e.target.value)}
          />
          {id ? (
            <img className="youtube-dialog-preview" src={thumbnailUrl(id)} alt="" />
          ) : (
            <p className={url.trim() ? 'error m-0' : 'muted small m-0'}>
              {url.trim() ? 'YouTube の動画の URL として読み取れません' : '動画のページ（共有ボタンの URL でも可）の URL を貼り付けます。'}
            </p>
          )}
        </div>
        <div className="modal-footer">
          <button type="button" className="btn btn-outline-secondary" onClick={onClose}>
            キャンセル
          </button>
          <button type="submit" className="btn btn-primary" disabled={!id}>
            埋め込む
          </button>
        </div>
      </form>
    </Modal>
  )
}
