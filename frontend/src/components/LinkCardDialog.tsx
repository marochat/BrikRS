import { useState, type ChangeEvent, type FormEvent } from 'react'
import { api } from '../api'
import { EditorModal } from './Modal'

export type ImagePos = 'left' | 'right' | 'top'

/** リンクカードの設定。title / description は作るときだけ（作った後はカードの上で直接書く） */
export type LinkCardForm = { url: string; image: string; imagePos: ImagePos; title: string; description: string }

const POSITIONS: { value: ImagePos; label: string }[] = [
  { value: 'left', label: '左' },
  { value: 'right', label: '右' },
  { value: 'top', label: '上' },
]

type Props = {
  initial: LinkCardForm
  /** 作るとき（タイトル・説明の欄を出す）か、作ったカードの設定を変えるときか */
  mode: 'create' | 'edit'
  onDone: (f: LinkCardForm) => void
  onClose: () => void
}

/** リンクカードを作る・設定を変えるダイアログ。OGP が取れればタイトル・説明・画像の下書きに使う */
export default function LinkCardDialog({ initial, mode, onDone, onClose }: Props) {
  const [f, setF] = useState(initial)
  const [busy, setBusy] = useState<'ogp' | 'upload' | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)

  const change = (patch: Partial<LinkCardForm>) => setF((x) => ({ ...x, ...patch }))

  const loadOgp = async () => {
    setBusy('ogp')
    setError(null)
    setNote(null)
    try {
      const b = await api.bookmark(f.url.trim())
      setF((x) => ({
        ...x,
        url: b.url,
        image: b.image || x.image,
        // 作った後はタイトル・説明をカードの上で書くので、ここでは画像だけ使う
        ...(mode === 'create' ? { title: b.title || x.title, description: b.description || x.description } : {}),
      }))
      if (!b.image) setNote('画像は取れませんでした。自分で選べます。')
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(null)
    }
  }

  const upload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    setBusy('upload')
    setError(null)
    try {
      change({ image: (await api.uploadImage(file)).url })
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(null)
    }
  }

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (busy || !f.url.trim()) return
    onDone({ ...f, url: f.url.trim(), title: f.title.trim(), description: f.description.trim() })
  }

  return (
    <EditorModal onClose={onClose}>
      <form onSubmit={submit}>
        <div className="modal-header">
          <h2 className="modal-title fs-5 brand-font">{mode === 'create' ? 'リンクカードを作る' : 'リンクカードの設定'}</h2>
          <button type="button" className="btn-close" aria-label="閉じる" onClick={onClose} />
        </div>
        <div className="modal-body vstack gap-3">
          <div>
            <label className="form-label" htmlFor="lc-url">
              リンク先の URL
            </label>
            <div className="input-group">
              <input
                id="lc-url"
                className="form-control"
                type="url"
                inputMode="url"
                placeholder="https://..."
                value={f.url}
                autoFocus={mode === 'create'}
                onChange={(e) => change({ url: e.target.value })}
              />
              <button
                type="button"
                className="btn btn-outline-secondary"
                disabled={!!busy || !f.url.trim()}
                onClick={loadOgp}
              >
                {busy === 'ogp' ? '読み込み中…' : mode === 'create' ? 'ページから読み込む' : '画像を読み込む'}
              </button>
            </div>
            <div className="form-text">
              {mode === 'create'
                ? '読み込むと、ページのタイトル・説明・画像を下書きにします（取れない分は自分で書けます）。'
                : 'ページの OGP 画像を読み込み直します。'}
            </div>
          </div>
          {mode === 'create' && (
            <>
              <label className="form-label mb-0">
                タイトル
                <input className="form-control mt-1" value={f.title} onChange={(e) => change({ title: e.target.value })} />
              </label>
              <label className="form-label mb-0">
                説明（カードの本文の最初の段落になります）
                <textarea
                  className="form-control mt-1"
                  rows={2}
                  value={f.description}
                  onChange={(e) => change({ description: e.target.value })}
                />
              </label>
            </>
          )}
          <div>
            <div className="form-label">画像</div>
            <div className="d-flex align-items-start gap-3 flex-wrap">
              <div className="link-card-dialog-image">
                {f.image ? <img src={f.image} alt="" /> : <span className="muted small">画像なし</span>}
              </div>
              <div className="vstack gap-2 flex-grow-0">
                <label className="btn btn-sm btn-outline-primary mb-0">
                  {busy === 'upload' ? 'アップロード中…' : '画像を選ぶ'}
                  <input type="file" accept="image/png,image/jpeg,image/gif,image/webp" hidden onChange={upload} />
                </label>
                {f.image && (
                  <button type="button" className="btn btn-sm btn-outline-secondary" onClick={() => change({ image: '' })}>
                    画像を外す
                  </button>
                )}
              </div>
            </div>
          </div>
          {f.image && (
            <div>
              <div className="form-label">画像の位置</div>
              <div className="btn-group" role="radiogroup" aria-label="画像の位置">
                {POSITIONS.map((p) => (
                  <button
                    key={p.value}
                    type="button"
                    role="radio"
                    aria-checked={f.imagePos === p.value}
                    className={`btn btn-outline-primary${f.imagePos === p.value ? ' active' : ''}`}
                    onClick={() => change({ imagePos: p.value })}
                  >
                    {p.label}
                  </button>
                ))}
              </div>
            </div>
          )}
          {note && <p className="muted small m-0">{note}</p>}
          {error && <p className="error m-0">{error}</p>}
        </div>
        <div className="modal-footer">
          <button type="button" className="btn btn-outline-secondary" onClick={onClose}>
            キャンセル
          </button>
          <button type="submit" className="btn btn-primary" disabled={!!busy || !f.url.trim()}>
            {mode === 'create' ? '作る' : '保存'}
          </button>
        </div>
      </form>
    </EditorModal>
  )
}
