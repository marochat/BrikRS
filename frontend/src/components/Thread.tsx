import { lazy, Suspense, useState, type ReactNode } from 'react'
import { accessBadge, canManage } from '../access'
import { api, type PageDetail, type PostNode } from '../api'
import { useApp } from '../AppContext'
import { navigate, pagePath } from '../router'
import { ago } from '../time'
import BlockView from './BlockView'
import { ReactionBar } from './Interactions'

// 返信欄は BlockNote を使うので、編集者が開いたときに初めて読み込む
const QuickPost = lazy(() => import('./Editor').then((m) => ({ default: m.QuickPost })))

/** これより深い返信は字下げしない（狭い画面で本文が細くなりすぎないように） */
const MAX_INDENT = 4
/** 返信がこれより多い投稿は、最初は畳んでおく */
const OPEN_MAX_REPLIES = 3
const COLLAPSE_KEY = 'brikrs.thread'

/** 開閉を自分で切り替えた投稿（id → 開いているか）。選んでいなければ件数で決める */
function loadCollapse(): Record<number, boolean> {
  try {
    return JSON.parse(localStorage.getItem(COLLAPSE_KEY) ?? '{}')
  } catch {
    return {}
  }
}

function saveCollapse(id: number, open: boolean) {
  try {
    localStorage.setItem(COLLAPSE_KEY, JSON.stringify({ ...loadCollapse(), [id]: open }))
  } catch {
    /* 保存できなくても動作には影響しない */
  }
}

export const countReplies = (rs: PostNode[]): number => rs.reduce((n, r) => n + 1 + countReplies(r.replies), 0)

/** 投稿・返信を、その下の返信ごとゴミ箱へ移す。確認で取り消したら false */
export async function trashPost(p: PostNode, what: '投稿' | '返信'): Promise<boolean> {
  // 手元の木はこの画面での追加・削除を反映していないことがあるので、件数はサーバーに聞き直す
  const replies = await api
    .thread(p.id)
    .then((t) => t.replies)
    .catch(() => p.replies)
  const n = countReplies(replies)
  const msg = n ? `この${what}と、その下の返信 ${n} 件をゴミ箱へ移します。` : `この${what}をゴミ箱へ移します。`
  if (!confirm(`${msg}（ゴミ箱から復元できます）`)) return false
  try {
    await api.deletePage(p.id)
    return true
  } catch (e) {
    alert((e as Error).message)
    return false
  }
}

/** 投稿・返信のロックを切り替え、切り替えた後の値を返す（失敗したら null） */
async function toggleLock(p: PageDetail): Promise<boolean | null> {
  try {
    return (await api.updateSettings(p.id, { is_locked: !p.is_locked })).is_locked
  } catch (e) {
    alert((e as Error).message)
    return null
  }
}

/**
 * 見出しに置く錠前。編集モードで作った人・管理者なら押して掛け外しできる（🔐 ロック中 / 🔓 ロックなし）。
 * それ以外の人には、ロック中のときだけ印として出す
 */
export function LockButton({ p, onChange }: { p: PageDetail; onChange: (locked: boolean) => void }) {
  const { me, editing } = useApp()
  if (!(editing && canManage(me, p.created_by_id))) {
    return p.is_locked ? (
      <span className="badge text-bg-light border" title="ロック中（編集できるのは作った人と管理者だけ。ゴミ箱へは外してから）">
        🔐
      </span>
    ) : null
  }
  return (
    <button
      type="button"
      className={`btn btn-sm btn-link text-decoration-none p-0 timeline-lock${p.is_locked ? ' locked' : ''}`}
      title={p.is_locked ? 'ロック中：押すとロックを外します' : '押すとロックします（作った人と管理者のほかは閲覧だけ。移動・ゴミ箱もできなくなります）'}
      aria-pressed={p.is_locked}
      onClick={async () => {
        const l = await toggleLock(p)
        if (l !== null) onChange(l)
      }}
    >
      {p.is_locked ? '🔐' : '🔓'}
    </button>
  )
}

/** 見出しの右端に置く、編集者用のゴミ箱ボタン */
export function TrashButton({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="btn btn-sm btn-link text-decoration-none p-0 timeline-trash" title="ゴミ箱へ" onClick={onClick}>
      🗑
    </button>
  )
}

/**
 * 投稿・返信の見出し（投稿者、日時＝その投稿へのリンク、編集済み・下書き）。lead は先頭に、children は後ろに出す。
 * children に LockButton を入れるときは lockInChildren にする（ロックの印を二重に出さない）
 */
export function PostHead({
  p,
  lead,
  children,
  lockInChildren,
}: {
  p: PageDetail
  lead?: ReactNode
  children?: ReactNode
  lockInChildren?: boolean
}) {
  return (
    <header className="timeline-post-head">
      {lead}
      {/* 作成者が無い投稿（初期データ、削除されたユーザー）は名前を出さない */}
      {p.created_by && <span className="timeline-author">{p.created_by}</span>}
      <a
        className="timeline-time"
        href={pagePath(p.id)}
        title={new Date(p.created_at).toLocaleString('ja-JP')}
        onClick={(e) => (e.preventDefault(), navigate(pagePath(p.id)))}
      >
        {ago(p.created_at)}
      </a>
      {p.updated_at !== p.created_at && <span className="muted small">（編集済み）</span>}
      {p.is_draft && <span className="badge text-bg-secondary">下書き</span>}
      {!p.is_draft && accessBadge(p) && (
        <span className="badge text-bg-light border" title="公開範囲を限っています（返信も含む）">
          🔒 {accessBadge(p)}
        </span>
      )}
      {p.is_locked && !lockInChildren && (
        <span className="badge text-bg-light border" title="ロック中（編集できるのは作った人と管理者だけ。ゴミ箱へは外してから）">
          🔐
        </span>
      )}
      {children}
    </header>
  )
}

type Update = (f: (rs: PostNode[]) => PostNode[]) => void

/**
 * 投稿・返信の本文の下に置く：「返信」ボタン、返信欄、返信の木（新しい順、畳める）。
 * alwaysOpen はスレッド表示で、開いているページへの返信を常に見せるとき
 */
export default function Thread({
  id,
  replies: initial,
  alwaysOpen = false,
}: {
  id: number
  replies: PostNode[]
  alwaysOpen?: boolean
}) {
  // 木全体はここで持つ。下の段で足した返信もここへ反映するので、畳んで開き直しても消えない
  const [replies, setReplies] = useState(initial)
  return <Replies id={id} replies={replies} update={setReplies} depth={0} alwaysOpen={alwaysOpen} />
}

function Replies({
  id,
  replies,
  update,
  depth,
  alwaysOpen = false,
}: {
  id: number
  replies: PostNode[]
  update: Update
  depth: number
  alwaysOpen?: boolean
}) {
  const { editing, reloadPages } = useApp()
  const [composing, setComposing] = useState(false)
  const total = countReplies(replies)
  const [open, setOpen] = useState(() => alwaysOpen || (loadCollapse()[id] ?? total <= OPEN_MAX_REPLIES))

  const toggle = () => {
    saveCollapse(id, !open)
    setOpen(!open)
  }

  const onPosted = (p: PageDetail) => {
    update((rs) => [{ ...p, replies: [] }, ...rs])
    setComposing(false)
    setOpen(true)
  }

  const remove = async (r: PostNode) => {
    if (!(await trashPost(r, '返信'))) return
    update((rs) => rs.filter((x) => x.id !== r.id))
    reloadPages()
  }

  if (!editing && replies.length === 0) return null

  return (
    <div className="thread">
      <div className="thread-actions">
        {replies.length > 0 && !alwaysOpen && (
          <button type="button" className="btn btn-sm btn-link text-decoration-none p-0" aria-expanded={open} onClick={toggle}>
            {open ? '▾' : '▸'} 返信 {total}件
          </button>
        )}
        {editing && !composing && (
          <button type="button" className="btn btn-sm btn-link text-decoration-none p-0" onClick={() => setComposing(true)}>
            💬 返信する
          </button>
        )}
      </div>
      {composing && (
        <Suspense fallback={<div className="card shadow-sm quick-post quick-post-loading" />}>
          <QuickPost
            parentId={id}
            onPosted={onPosted}
            onCancel={() => setComposing(false)}
            submitLabel="返信"
            autoFocus
          />
        </Suspense>
      )}
      {open && replies.length > 0 && (
        <ol className={`thread-replies${depth >= MAX_INDENT ? ' flat' : ''}`}>
          {replies.map((r) => (
            <li key={r.id} className="thread-reply">
              <PostHead p={r} lockInChildren>
                <LockButton
                  p={r}
                  onChange={(l) => update((rs) => rs.map((x) => (x.id === r.id ? { ...x, is_locked: l } : x)))}
                />
                {editing && !r.is_locked && <TrashButton onClick={() => remove(r)} />}
              </PostHead>
              <BlockView blocks={r.content} />
              <ReactionBar id={r.id} />
              <Replies
                id={r.id}
                replies={r.replies}
                update={(f) => update((rs) => rs.map((x) => (x.id === r.id ? { ...x, replies: f(x.replies) } : x)))}
                depth={depth + 1}
              />
            </li>
          ))}
        </ol>
      )}
    </div>
  )
}
