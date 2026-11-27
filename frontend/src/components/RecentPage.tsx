import { useEffect, useState } from 'react'
import { api, type RecentItem } from '../api'
import { displayTitle } from '../AppContext'
import { navigate, pagePath } from '../router'
import { ago } from '../time'

export default function RecentPage() {
  const [items, setItems] = useState<RecentItem[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    document.title = '最近の更新'
    api.recent().then(setItems).catch((e) => setError(e.message))
  }, [])

  return (
    <div className="admin vstack gap-3">
      <div className="d-flex align-items-center justify-content-between">
        <h1 className="brand-font fs-3 m-0">最近の更新</h1>
        <a className="btn btn-sm btn-outline-secondary" href="/feed.atom" title="Atom フィード">
          フィード
        </a>
      </div>
      {error && <p className="error">{error}</p>}
      {items?.length === 0 && <p className="muted">まだページがありません</p>}
      {!!items?.length && (
        <ul className="list-group shadow-sm recent-list">
          {items.map((it) => (
            <li key={it.id} className="list-group-item">
              <a
                className="recent-title"
                href={pagePath(it.id)}
                onClick={(e) => (e.preventDefault(), navigate(pagePath(it.id)))}
              >
                <span className="tree-icon">{it.icon || '📄'}</span>
                {displayTitle(it.title)}
                {it.is_draft && <span className="badge text-bg-secondary ms-2">下書き</span>}
              </a>
              <span className="recent-meta" title={new Date(it.updated_at).toLocaleString('ja-JP')}>
                {ago(it.updated_at)}
                {it.updated_by && ` · ${it.updated_by}`}
              </span>
              {it.snippet && <p className="recent-snippet">{it.snippet}</p>}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
