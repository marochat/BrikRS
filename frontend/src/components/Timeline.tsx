import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react'
import { api, type ArchiveMonth, type PageDetail, type PostNode } from '../api'
import { lockedForMe } from '../access'
import { useApp } from '../AppContext'
import BlockView from './BlockView'
import { InteractionsContext, useInteractionsStore } from '../interactions'
import { GuestComments, ReactionBar } from './Interactions'
import Thread, { LockButton, PostHead, TrashButton, trashPost } from './Thread'

// 投稿欄は BlockNote を使うので、編集者にだけ後から読み込む
const QuickPost = lazy(() => import('./Editor').then((m) => ({ default: m.QuickPost })))

const MONTH_RE = /^\d{4}-\d{2}$/

/** 投稿日時の、閲覧している人の暦での年月（YYYY-MM） */
const monthOf = (iso: string) => {
  const d = new Date(iso)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`
}

const monthLabel = (m: string) => `${Number(m.slice(0, 4))}年${Number(m.slice(5))}月`

/** URL の ?month=YYYY-MM（月別アーカイブ） */
const monthFromUrl = () => {
  const m = new URLSearchParams(location.search).get('month')
  return m && MONTH_RE.test(m) ? m : null
}

/** タイムライン表示：子ページを投稿として新しい順に並べ、その下に返信の木を出す。月別アーカイブで絞り込める */
export default function Timeline({ pageId }: { pageId: number }) {
  const { editing } = useApp()
  const [month, setMonth] = useState<string | null>(monthFromUrl)
  const [archive, setArchive] = useState<ArchiveMonth[]>([])
  /** 投稿欄から投稿したもの（新しい順。一覧の先頭に足す） */
  const [posted, setPosted] = useState<PostNode[]>([])

  const loadArchive = useCallback(
    () =>
      api
        .archive(pageId)
        .then(setArchive)
        .catch(() => setArchive([])),
    [pageId],
  )
  useEffect(() => {
    loadArchive()
  }, [loadArchive])

  // 選んだ月は URL に残す（共有・再読み込みでも同じ月を開く）
  const changeMonth = (m: string | null) => {
    setMonth(m)
    history.replaceState(null, '', m ? `${location.pathname}?month=${m}` : location.pathname)
  }

  const onPosted = (p: PageDetail) => {
    // 別の月を見ているときは、投稿が見えるように絞り込みを外す
    if (month && month !== monthOf(p.created_at)) changeMonth(null)
    setPosted((ps) => [{ ...p, replies: [] }, ...ps])
    loadArchive()
  }

  // 年ごとにまとめる（新しい年から）
  const years = [...new Set(archive.map((a) => a.month.slice(0, 4)))]
  const total = archive.reduce((n, a) => n + a.count, 0)

  return (
    <section className="timeline">
      {editing && (
        <Suspense fallback={<div className="card shadow-sm quick-post quick-post-loading" />}>
          <QuickPost parentId={pageId} onPosted={onPosted} chooseAccess />
        </Suspense>
      )}
      {archive.length > 0 && (
        <div className="timeline-archive">
          <label htmlFor="timeline-month" className="muted small">
            📅 アーカイブ
          </label>
          <select
            id="timeline-month"
            className="form-select form-select-sm"
            value={month ?? ''}
            onChange={(e) => changeMonth(e.target.value || null)}
          >
            <option value="">すべての投稿（{total}）</option>
            {/* URL で指定された月に投稿が無くても、選択中として出す */}
            {month && !archive.some((a) => a.month === month) && <option value={month}>{monthLabel(month)}（0）</option>}
            {years.map((y) => (
              <optgroup key={y} label={`${y}年`}>
                {archive
                  .filter((a) => a.month.startsWith(y))
                  .map((a) => (
                    <option key={a.month} value={a.month}>
                      {monthLabel(a.month)}（{a.count}）
                    </option>
                  ))}
              </optgroup>
            ))}
          </select>
          {month && (
            <button type="button" className="btn btn-sm btn-link text-decoration-none p-0" onClick={() => changeMonth(null)}>
              すべて表示
            </button>
          )}
        </div>
      )}
      <PostList key={month ?? ''} pageId={pageId} month={month} posted={posted} />
    </section>
  )
}

/** 投稿の一覧。スクロールで続きを読み込む。月を変えたら key で作り直す */
function PostList({ pageId, month, posted }: { pageId: number; month: string | null; posted: PostNode[] }) {
  const { me, editing, reloadPages } = useApp()
  /** この画面でゴミ箱へ移した投稿（投稿欄から足したものも消せるよう、id で除く） */
  const [removed, setRemoved] = useState<Set<number>>(new Set())
  const [pinned, setPinned] = useState<PostNode[]>([])
  const [posts, setPosts] = useState<PostNode[]>([])
  const [hasMore, setHasMore] = useState(true)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const sentinel = useRef<HTMLDivElement>(null)
  /** ゲストコメント・リアクションの受け付けと件数 */
  const interactions = useInteractionsStore()
  const { merge } = interactions

  const loadMore = useCallback(async () => {
    if (loading || !hasMore) return
    setLoading(true)
    try {
      const before = posts.length ? posts[posts.length - 1].id : undefined
      const r = await api.posts(pageId, { before, month })
      if (before === undefined) setPinned(r.pinned)
      setPosts((ps) => [...ps, ...r.posts.filter((p) => !ps.some((x) => x.id === p.id))])
      setHasMore(r.has_more)
      merge(r.interactions)
    } catch (e) {
      setError((e as Error).message)
      setHasMore(false)
    } finally {
      setLoading(false)
    }
  }, [pageId, month, posts, loading, hasMore, merge])

  // 一覧の末尾が見えたら続きを読み込む
  useEffect(() => {
    const el = sentinel.current
    if (!el) return
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && loadMore(), {
      rootMargin: '400px',
    })
    io.observe(el)
    return () => io.disconnect()
  }, [loadMore])

  const byNewest = (a: PostNode, b: PostNode) => b.id - a.id

  /** ピン留め / 解除。解除した投稿は、読み込み済みの範囲に入るなら元の位置へ戻す */
  const togglePin = async (p: PostNode) => {
    try {
      const u = { ...p, is_pinned: (await api.updateSettings(p.id, { is_pinned: !p.is_pinned })).is_pinned }
      if (month) {
        // 月別の表示ではピン留めも日付順のまま並べる
        setPosts((ps) => ps.map((x) => (x.id === u.id ? u : x)))
      } else if (u.is_pinned) {
        setPosts((ps) => ps.filter((x) => x.id !== u.id))
        setPinned((ps) => [...ps, u].sort(byNewest))
      } else {
        setPinned((ps) => ps.filter((x) => x.id !== u.id))
        setPosts((ps) =>
          !hasMore || (ps.length && u.id > ps[ps.length - 1].id) ? [...ps, u].sort(byNewest) : ps,
        )
      }
    } catch (e) {
      alert((e as Error).message)
    }
  }

  /**
   * この画面でロックを切り替えた投稿（id → ロック中か）。投稿欄から投稿したばかりの投稿は親（Timeline）の
   * posted にあって、ここの一覧では書き換えられないので、表示するときに上書きする
   */
  const [lockOverride, setLockOverride] = useState<Record<number, boolean>>({})
  const setLocked = (id: number, is_locked: boolean) => setLockOverride((o) => ({ ...o, [id]: is_locked }))

  const remove = async (p: PostNode) => {
    if (!(await trashPost(p, '投稿'))) return
    setRemoved((s) => new Set(s).add(p.id))
    reloadPages()
  }

  const renderPost = (post: PostNode) => {
    if (removed.has(post.id)) return null
    const p = post.id in lockOverride ? { ...post, is_locked: lockOverride[post.id] } : post
    return (
      <article key={p.id} className={`card shadow-sm timeline-post${p.is_pinned ? ' pinned' : ''}`}>
        <PostHead p={p} lead={p.is_pinned && <span className="timeline-pin">📌 固定</span>} lockInChildren>
          {editing && !lockedForMe(me, p) && (
            <button
              type="button"
              className="btn btn-sm btn-link text-decoration-none ms-auto p-0 timeline-pin-toggle"
              onClick={() => togglePin(p)}
            >
              {p.is_pinned ? '固定を外す' : '📌 固定'}
            </button>
          )}
          <LockButton p={p} onChange={(l) => setLocked(p.id, l)} />
          {editing && !p.is_locked && <TrashButton onClick={() => remove(p)} />}
        </PostHead>
        <BlockView blocks={p.content} />
        <ReactionBar id={p.id} />
        <Thread id={p.id} replies={p.replies} />
        {/* 下書き保存・公開範囲を限って投稿したばかりの投稿は、まだ受け付けの一覧に入っていないので、ここでも外す */}
        {!p.is_draft && p.visibility === 'public' && <GuestComments postId={p.id} />}
      </article>
    )
  }

  // 投稿欄から投稿したもの（この一覧に入り、まだ読み込んでいないもの）を先頭に出す
  const fresh = posted.filter(
    (p) =>
      (!month || month === monthOf(p.created_at)) &&
      !posts.some((x) => x.id === p.id) &&
      !pinned.some((x) => x.id === p.id),
  )

  return (
    <InteractionsContext.Provider value={interactions}>
      {pinned.map(renderPost)}
      {fresh.map(renderPost)}
      {posts.map(renderPost)}
      {error && <p className="error">{error}</p>}
      {!loading && !hasMore && posts.length + pinned.length + fresh.length === 0 && (
        <p className="muted text-center py-4">{month ? `${monthLabel(month)}の投稿はありません` : 'まだ投稿がありません'}</p>
      )}
      <div ref={sentinel} className="timeline-sentinel">
        {loading && <span className="muted small">読み込み中…</span>}
        {!loading && hasMore && posts.length > 0 && (
          <button className="btn btn-sm btn-outline-secondary" onClick={loadMore}>
            もっと見る
          </button>
        )}
      </div>
    </InteractionsContext.Provider>
  )
}
