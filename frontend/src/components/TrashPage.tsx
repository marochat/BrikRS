import { useEffect, useState } from 'react'
import { api, type TrashItem } from '../api'
import { displayTitle, useApp } from '../AppContext'
import { navigate, pagePath } from '../router'

export default function TrashPage() {
  const { setPages } = useApp()
  const [items, setItems] = useState<TrashItem[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = () =>
    api
      .listTrash()
      .then(setItems)
      .catch((e) => setError(e.message))
  useEffect(() => {
    load()
  }, [])

  const run = async (fn: () => Promise<unknown>) => {
    setError(null)
    try {
      await fn()
      await load()
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const restore = (t: TrashItem) =>
    run(async () => {
      const r = await api.restoreTrash(t.id)
      setPages(r.pages)
      if (r.made_draft) {
        alert(
          `「${displayTitle(t.title)}」は親ページがゴミ箱にあるため最上位に戻しました。` +
            '親ページの下書き・公開範囲の制限が外れないよう、下書き（作った人と管理者だけに見える）にしてあります。',
        )
      }
      // 下書きにして自分に見えなくなった（他の人が作ったページ）ときは、開かない
      if (r.pages.some((p) => p.id === t.id)) navigate(pagePath(t.id))
    })

  const purge = (t: TrashItem) => {
    const extra = t.descendants ? `と子ページ ${t.descendants} 件` : ''
    if (confirm(`「${displayTitle(t.title)}」${extra}を完全に削除します。編集履歴も消え、元に戻せません。`)) {
      run(() => api.purgeTrash(t.id))
    }
  }

  const empty = () => {
    if (confirm('ゴミ箱を空にします。すべて完全に削除され、元に戻せません。')) run(() => api.emptyTrash())
  }

  return (
    <div className="admin vstack gap-3">
      <div className="d-flex align-items-center justify-content-between">
        <h1 className="brand-font fs-3 m-0">ゴミ箱</h1>
        {!!items?.length && (
          <button className="btn btn-sm btn-outline-danger" onClick={empty}>
            ゴミ箱を空にする
          </button>
        )}
      </div>
      {error && <p className="error">{error}</p>}
      {items?.length === 0 && <p className="muted">ゴミ箱は空です</p>}
      {!!items?.length && (
        <ul className="list-group shadow-sm trash-list">
          {items.map((t) => (
            <li key={t.id} className="list-group-item">
              <span className="tree-icon">{t.icon || '📄'}</span>
              <span className="trash-title">
                {displayTitle(t.title)}
                {t.descendants > 0 && <span className="muted"> ＋子ページ {t.descendants} 件</span>}
              </span>
              <span className="trash-meta muted">
                {new Date(t.deleted_at).toLocaleString('ja-JP')}
                {t.deleted_by && ` · ${t.deleted_by}`}
              </span>
              <span className="trash-actions">
                <button className="btn btn-sm btn-outline-secondary" onClick={() => restore(t)}>
                  復元
                </button>
                <button className="btn btn-sm btn-outline-danger" onClick={() => purge(t)}>
                  完全に削除
                </button>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
