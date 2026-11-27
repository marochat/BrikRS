import { linkHost as host } from '../linkHost'

/** Web ブックマークのカード（閲覧画面と編集画面で共通。BlockNote には依存しない） */
export type BookmarkProps = { url: string; title: string; description: string; siteName: string; image: string }


/**
 * link なら全体をリンクにする（閲覧画面）。編集画面ではクリックがブロックの選択に使われるので、
 * 右上の「↗」だけをリンクにする
 */
export default function BookmarkCard({ b, link }: { b: BookmarkProps; link: boolean }) {
  const body = (
    <>
      <span className="bookmark-text">
        <span className="bookmark-title">{b.title || host(b.url)}</span>
        {b.description && <span className="bookmark-desc">{b.description}</span>}
        <span className="bookmark-site">
          {b.siteName && b.siteName !== host(b.url) ? `${b.siteName} · ` : ''}
          {host(b.url)}
        </span>
      </span>
      {b.image && (
        <span className="bookmark-image">
          <img src={b.image} alt="" loading="lazy" />
        </span>
      )}
    </>
  )
  if (link) {
    return (
      <a className="bookmark-card" href={b.url} target="_blank" rel="noopener noreferrer">
        {body}
      </a>
    )
  }
  return (
    <div className="bookmark-card">
      {body}
      <a className="bookmark-open" href={b.url} target="_blank" rel="noopener noreferrer" title="新しいタブで開く">
        ↗
      </a>
    </div>
  )
}
