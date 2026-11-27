import { useCallback, useEffect, useState } from 'react'
import { api, type AdminComment } from '../../api'
import { navigate, pagePath } from '../../router'

type Status = 'pending' | 'approved'

/** 管理画面の「コメント」タブ：承認待ちのゲストコメントの承認・削除（却下）、公開中のコメントの一覧 */
export default function CommentsPanel({ onChanged }: { onChanged?: () => void }) {
  const [status, setStatus] = useState<Status>('pending')
  const [items, setItems] = useState<AdminComment[]>([])
  const [hasMore, setHasMore] = useState(false)
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(
    async (before?: number) => {
      setError(null)
      try {
        const r = await api.adminComments(status, before)
        setItems((cur) => (before ? [...cur, ...r.items] : r.items))
        setHasMore(r.has_more)
      } catch (e) {
        setError((e as Error).message)
      }
    },
    [status],
  )

  useEffect(() => {
    setItems([])
    setSelected(new Set())
    load()
  }, [load])

  const drop = (ids: number[]) => {
    setItems((cur) => cur.filter((c) => !ids.includes(c.id)))
    setSelected((s) => new Set([...s].filter((x) => !ids.includes(x))))
    onChanged?.()
  }

  const run = async (ids: number[], action: 'approve' | 'delete') => {
    if (ids.length === 0) return
    if (action === 'delete' && !confirm(`${ids.length} 件のコメントを削除します（元に戻せません）。`)) return
    setBusy(true)
    try {
      if (ids.length === 1) {
        await (action === 'approve' ? api.approveComment(ids[0]) : api.deleteComment(ids[0]))
      } else {
        await api.bulkComments(ids, action)
      }
      drop(ids)
    } catch (e) {
      alert((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const toggle = (id: number) =>
    setSelected((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })
  const allSelected = items.length > 0 && items.every((c) => selected.has(c.id))

  return (
    <div className="card shadow-sm">
      <div className="card-header d-flex flex-wrap align-items-center gap-2">
        <div className="btn-group btn-group-sm" role="group" aria-label="表示するコメント">
          {(
            [
              ['pending', '承認待ち'],
              ['approved', '公開中'],
            ] as const
          ).map(([s, label]) => (
            <button
              key={s}
              type="button"
              className={`btn btn-outline-secondary${status === s ? ' active' : ''}`}
              onClick={() => setStatus(s)}
            >
              {label}
            </button>
          ))}
        </div>
        {items.length > 0 && (
          <label className="form-check-label small ms-2">
            <input
              type="checkbox"
              className="form-check-input me-1"
              checked={allSelected}
              onChange={() => setSelected(allSelected ? new Set() : new Set(items.map((c) => c.id)))}
            />
            すべて選択
          </label>
        )}
        <div className="ms-auto d-flex gap-2">
          {status === 'pending' && (
            <button
              type="button"
              className="btn btn-sm btn-primary"
              disabled={busy || selected.size === 0}
              onClick={() => run([...selected], 'approve')}
            >
              選んだものを承認
            </button>
          )}
          <button
            type="button"
            className="btn btn-sm btn-outline-danger"
            disabled={busy || selected.size === 0}
            onClick={() => run([...selected], 'delete')}
          >
            選んだものを削除
          </button>
        </div>
      </div>
      <div className="card-body">
        <p className="form-text mt-0">
          承認するまで公開されません。却下するときは削除してください。IP アドレスなどは90日で消えます（承認されないままのコメントも90日で削除されます）。
        </p>
        {error && <p className="error">{error}</p>}
        {items.length === 0 && !error && (
          <p className="muted text-center py-3 m-0">{status === 'pending' ? '承認待ちのコメントはありません' : '公開中のコメントはありません'}</p>
        )}
        <ul className="admin-comment-list">
          {items.map((c) => (
            <li key={c.id} className="admin-comment">
              <div className="admin-comment-head">
                <input
                  type="checkbox"
                  className="form-check-input"
                  aria-label="選ぶ"
                  checked={selected.has(c.id)}
                  onChange={() => toggle(c.id)}
                />
                <strong>{c.name}</strong>
                {c.is_staff && <span className="badge text-bg-primary">管理人</span>}
                {c.parent_id && <span className="badge text-bg-light">返信</span>}
                {c.link_count > 0 && <span className="badge text-bg-warning">リンク {c.link_count}</span>}
                <span className="muted small" title={c.created_at}>
                  {new Date(c.created_at).toLocaleString('ja-JP')}
                </span>
                <a
                  className="small text-truncate admin-comment-page"
                  href={pagePath(c.page_id)}
                  onClick={(e) => (e.preventDefault(), navigate(pagePath(c.page_id)))}
                >
                  {c.page_icon} {c.page_title || '無題'}
                </a>
              </div>
              <p className="admin-comment-body">{c.body}</p>
              <div className="admin-comment-foot">
                <span className="muted small text-truncate" title={c.user_agent ?? undefined}>
                  {c.ip ?? 'IP は消去済み'}
                  {c.user_agent && ` · ${c.user_agent}`}
                </span>
                <div className="ms-auto d-flex gap-2">
                  {status === 'pending' && (
                    <button type="button" className="btn btn-sm btn-primary" disabled={busy} onClick={() => run([c.id], 'approve')}>
                      承認
                    </button>
                  )}
                  <button type="button" className="btn btn-sm btn-outline-danger" disabled={busy} onClick={() => run([c.id], 'delete')}>
                    削除
                  </button>
                </div>
              </div>
            </li>
          ))}
        </ul>
        {hasMore && (
          <div className="text-center">
            <button type="button" className="btn btn-sm btn-outline-secondary" onClick={() => load(items[items.length - 1]?.id)}>
              もっと見る
            </button>
          </div>
        )}
      </div>
    </div>
  )
}
