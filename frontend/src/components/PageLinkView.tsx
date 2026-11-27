import { displayTitle, useApp } from '../AppContext'
import { navigate, pagePath } from '../router'

/** 本文中のページリンクの表示（編集画面と閲覧画面で共用）。最新のタイトル・アイコンを引く */
export default function PageLinkView({ pageId }: { pageId: number }) {
  const { pages } = useApp()
  const p = pages.find((x) => x.id === pageId)
  if (!p) return <span className="page-ref missing">削除されたページ</span>
  return (
    <a
      className="page-ref"
      href={pagePath(p.id)}
      onClick={(e) => {
        e.preventDefault()
        navigate(pagePath(p.id))
      }}
    >
      <span className="page-ref-icon">{p.icon || '📄'}</span>
      {displayTitle(p.title)}
    </a>
  )
}
