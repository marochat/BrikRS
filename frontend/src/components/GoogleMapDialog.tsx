import { useState, type FormEvent } from 'react'
import { parseMap, type MapProps } from '../gmaps'
import GoogleMapEmbed from './GoogleMapEmbed'
import Modal from './Modal'

/** Google マップの URL か埋め込みコードを入力するダイアログ。読み取れたら下に地図を出し、onDone に渡す */
export default function GoogleMapDialog({ onDone, onClose }: { onDone: (map: MapProps) => void; onClose: () => void }) {
  const [input, setInput] = useState('')
  const map = parseMap(input)
  const short = /^\s*(https?:\/\/)?(maps\.app\.goo\.gl|goo\.gl)\//i.test(input)

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (map) onDone(map)
  }

  return (
    <Modal onClose={onClose}>
      <form onSubmit={submit}>
        <div className="modal-header">
          <h2 className="modal-title fs-5 brand-font">Google マップ</h2>
          <button type="button" className="btn-close" aria-label="閉じる" onClick={onClose} />
        </div>
        <div className="modal-body vstack gap-2">
          <textarea
            className="form-control font-monospace small"
            rows={3}
            placeholder={'https://www.google.com/maps/place/...\nまたは <iframe src="https://www.google.com/maps/embed?pb=..."></iframe>'}
            value={input}
            autoFocus
            aria-label="地図の URL か埋め込みコード"
            onChange={(e) => setInput(e.target.value)}
          />
          {map ? (
            <div className="gmap-dialog-preview">
              <GoogleMapEmbed map={map} />
            </div>
          ) : (
            <p className={input.trim() ? 'error m-0' : 'muted small m-0'}>
              {short
                ? '短縮 URL（maps.app.goo.gl）は使えません。ブラウザで開いたあとのアドレスバーの URL か、「共有」→「地図を埋め込む」のコードを貼り付けます。'
                : input.trim()
                  ? 'Google マップの URL・埋め込みコードとして読み取れません'
                  : '場所のページの URL、「共有」→「地図を埋め込む」のコード、マイマップの URL（または「自分のサイトに埋め込む」のコード）を貼り付けます。マイマップは「リンクを知っている全員」に公開しておく必要があります。'}
            </p>
          )}
        </div>
        <div className="modal-footer">
          <button type="button" className="btn btn-outline-secondary" onClick={onClose}>
            キャンセル
          </button>
          <button type="submit" className="btn btn-primary" disabled={!map}>
            埋め込む
          </button>
        </div>
      </form>
    </Modal>
  )
}
