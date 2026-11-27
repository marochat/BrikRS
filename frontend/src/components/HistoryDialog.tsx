import { useEffect, useState } from 'react'
import { api, type PageDetail, type Revision, type RevisionSummary } from '../api'
import { displayTitle } from '../AppContext'
import BlockView from './BlockView'
import Modal from './Modal'

type Props = {
  pageId: number
  onClose: () => void
  onRestored: (page: PageDetail) => void
  /** false なら見るだけ（ロック中のページで、作った人・管理者でないとき） */
  canRestore?: boolean
}

const fmt = (s: string) => new Date(s).toLocaleString('ja-JP')

export default function HistoryDialog({ pageId, onClose, onRestored, canRestore = true }: Props) {
  const [revs, setRevs] = useState<RevisionSummary[] | null>(null)
  const [current, setCurrent] = useState<Revision | null>(null)
  const [error, setError] = useState<string | null>(null)

  const select = (id: number) => api.getRevision(id).then(setCurrent).catch((e) => setError(e.message))

  useEffect(() => {
    api
      .listRevisions(pageId)
      .then((rs) => {
        setRevs(rs)
        if (rs[0]) select(rs[0].id)
      })
      .catch((e) => setError(e.message))
  }, [pageId])

  const restore = async () => {
    if (!current) return
    if (!confirm(`${fmt(current.updated_at)} の版に戻しますか？（今の内容も履歴に残ります）`)) return
    try {
      onRestored(await api.restoreRevision(current.id))
      onClose()
    } catch (e) {
      setError((e as Error).message)
    }
  }

  return (
    <Modal onClose={onClose} className="history-modal" dialogClassName="modal-xl modal-fullscreen-lg-down">
      <aside className="history-list">
        <div className="d-flex align-items-center justify-content-between mb-2 px-2">
          <h2 className="brand-font m-0">編集履歴</h2>
          <button type="button" className="btn-close d-lg-none" onClick={onClose} aria-label="閉じる" />
        </div>
        {revs?.length === 0 && <p className="muted">まだ履歴がありません</p>}
        <ul>
          {revs?.map((r, i) => (
            <li key={r.id}>
              <button className={current?.id === r.id ? 'selected' : ''} onClick={() => select(r.id)}>
                <span className="history-time">
                  {fmt(r.updated_at)}
                  {i === 0 && <span className="badge text-bg-primary ms-2">最新</span>}
                </span>
                <span className="history-who">{r.created_by ?? '（削除されたユーザー）'}</span>
              </button>
            </li>
          ))}
        </ul>
      </aside>
      <section className="history-preview">
        {error && <p className="error">{error}</p>}
        {current && (
          <>
            <div className="history-preview-bar">
              <span className="muted">
                {fmt(current.started_at)} 〜 {fmt(current.updated_at)} の編集
              </span>
              {canRestore && (
                <button className="btn btn-sm btn-primary text-nowrap" onClick={restore} disabled={current.id === revs?.[0]?.id}>
                  この版に戻す
                </button>
              )}
            </div>
            <h1 className="history-title">
              {current.icon && <span>{current.icon} </span>}
              {displayTitle(current.title)}
            </h1>
            <div className="history-content">
              <BlockView key={current.id} blocks={current.content} />
            </div>
          </>
        )}
      </section>
    </Modal>
  )
}
