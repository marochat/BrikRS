import { useCallback, useContext, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from 'react'
import {
  api,
  REACTION_KINDS,
  type CommentForm,
  type GuestComment,
  type InteractionStats,
  type ReactionKind,
} from '../api'
import { useApp } from '../AppContext'
import { InteractionsContext as Ctx } from '../interactions'
import { ago } from '../time'

// タイムラインの投稿へのリアクションとゲストコメント（承認制）。
// 受け付けるか・件数は、タイムラインの一覧やスレッドの応答に入っている interactions を Context で配る

const REACTIONS: Record<ReactionKind, { emoji: string; label: string }> = {
  like: { emoji: '👍', label: 'いいね' },
  heart: { emoji: '❤️', label: 'すき' },
  laugh: { emoji: '😂', label: 'わらった' },
  wow: { emoji: '😮', label: 'びっくり' },
  party: { emoji: '🎉', label: 'おめでとう' },
}

const NAME_MAX = 30
const BODY_MAX = 1000
/** フォーム用トークンの有効期間（15分）より少し早めに取り直す */
const FORM_REFRESH_MS = 14 * 60 * 1000
const NAME_KEY = 'brikrs.commentName'
const MINE_KEY = 'brikrs.myComments'

const NO_STATS: InteractionStats = { reactions: {}, mine: [], comments: 0 }

// ---------- 手元に残す自分のコメント ----------

type Mine = { token: string; page_id: number; name: string; body: string; created_at: string }

function loadMine(): Record<string, Mine> {
  try {
    return JSON.parse(localStorage.getItem(MINE_KEY) ?? '{}')
  } catch {
    return {}
  }
}

function saveMine(m: Record<string, Mine>) {
  try {
    localStorage.setItem(MINE_KEY, JSON.stringify(m))
  } catch {
    /* 保存できなくても投稿はできる（自分で消せなくなるだけ） */
  }
}

function loadName(): string {
  try {
    return localStorage.getItem(NAME_KEY) ?? ''
  } catch {
    return ''
  }
}

function saveName(n: string) {
  try {
    localStorage.setItem(NAME_KEY, n)
  } catch {
    /* 次回また入力してもらうだけ */
  }
}

// ---------- リアクション ----------

/** 投稿・返信の本文の下に置くリアクションの列。受け付けていなければ何も出さない */
export function ReactionBar({ id }: { id: number }) {
  const store = useContext(Ctx)
  const [picking, setPicking] = useState(false)
  const [busy, setBusy] = useState(false)
  if (!store || !store.inter.reactions) return null
  const stats = store.inter.items[id] ?? NO_STATS
  const used = REACTION_KINDS.filter((k) => (stats.reactions[k] ?? 0) > 0)

  const toggle = async (k: ReactionKind) => {
    if (busy) return
    setBusy(true)
    setPicking(false)
    try {
      const r = stats.mine.includes(k) ? await api.removeReaction(id, k) : await api.addReaction(id, k)
      const s = r.items[id]
      // コメントの数はリアクションの応答に入らないので、手元の数を残す
      store.setStats(id, s || stats.comments ? { ...(s ?? NO_STATS), comments: stats.comments } : undefined)
    } catch (e) {
      alert((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="reaction-bar" role="group" aria-label="リアクション">
      {used.map((k) => (
        <button
          key={k}
          type="button"
          className={`reaction-chip${stats.mine.includes(k) ? ' mine' : ''}`}
          aria-pressed={stats.mine.includes(k)}
          title={REACTIONS[k].label}
          disabled={busy}
          onClick={() => toggle(k)}
        >
          <span aria-hidden="true">{REACTIONS[k].emoji}</span>
          <span className="visually-hidden">{REACTIONS[k].label}</span> {stats.reactions[k]}
        </button>
      ))}
      <button
        type="button"
        className="reaction-chip reaction-add"
        aria-expanded={picking}
        title="リアクションを付ける"
        onClick={() => setPicking((p) => !p)}
      >
        ☺＋
      </button>
      {picking && (
        <span className="reaction-picker">
          {REACTION_KINDS.map((k) => (
            <button
              key={k}
              type="button"
              className={`reaction-pick${stats.mine.includes(k) ? ' mine' : ''}`}
              title={REACTIONS[k].label}
              aria-label={REACTIONS[k].label}
              aria-pressed={stats.mine.includes(k)}
              disabled={busy}
              onClick={() => toggle(k)}
            >
              {REACTIONS[k].emoji}
            </button>
          ))}
        </span>
      )}
    </div>
  )
}

// ---------- ゲストコメント ----------

/**
 * タイムラインの投稿の下に置くコメント欄。受け付けていなければ何も出さない。
 * 開くと一覧だけを出し、送信のフォームは「コメントを書く」を押したときに出す（CAPTCHA の画像もそのときに作る）。
 * openIfAny はスレッド表示用：公開中のコメントがあれば最初から開く
 */
export function GuestComments({ postId, openIfAny = false }: { postId: number; openIfAny?: boolean }) {
  const store = useContext(Ctx)
  const { me } = useApp()
  const [open, setOpen] = useState(() => openIfAny && (store?.inter.items[postId]?.comments ?? 0) > 0)
  const [writing, setWriting] = useState(false)
  const [comments, setComments] = useState<GuestComment[] | null>(null)
  const [mine, setMine] = useState<Record<string, Mine>>(loadMine)
  /** 手元のコメントのうち、承認待ちのもの（サーバーで確かめた結果） */
  const [pendingIds, setPendingIds] = useState<Set<number>>(new Set())
  const [error, setError] = useState<string | null>(null)

  // ゲストコメントは、ログインしていない人に見える投稿だけ（下書きには書ける人がいない）
  const enabled = !!store?.inter.comments && !store.inter.no_guest_comments?.includes(postId)
  const count = store?.inter.items[postId]?.comments ?? 0
  const myHere = useMemo(
    () => Object.entries(mine).filter(([, m]) => m.page_id === postId),
    [mine, postId],
  )

  const setCount = (n: number) => {
    if (!store) return
    const s = store.inter.items[postId] ?? NO_STATS
    store.setStats(postId, n || s.mine.length || Object.keys(s.reactions).length ? { ...s, comments: n } : undefined)
  }

  const load = useCallback(async () => {
    try {
      const r = await api.comments(postId)
      setComments(r.comments)
      // 手元の自分のコメントの状態を確かめ、消えたもの（却下・削除）は手元からも消す
      const all = loadMine()
      const here = Object.entries(all).filter(([, m]) => m.page_id === postId)
      if (here.length) {
        const st = await api.myComments(here.map(([id, m]) => ({ id: Number(id), token: m.token })))
        for (const [id] of here) if (!st[id]) delete all[id]
        saveMine(all)
        setMine(all)
        setPendingIds(new Set(Object.entries(st).filter(([, s]) => s === 'pending').map(([id]) => Number(id))))
      }
    } catch (e) {
      setError((e as Error).message)
    }
  }, [postId])

  useEffect(() => {
    if (open && enabled && comments === null) load()
  }, [open, enabled, comments, load])

  if (!enabled) return null

  const removeMine = async (id: number) => {
    const m = mine[id]
    if (!m || !confirm('このコメントを削除します。よろしいですか？')) return
    try {
      await api.deleteMyComment(id, m.token)
    } catch (e) {
      // もう消えている（却下された）ときも、手元からは消す
      if ((e as { status?: number }).status !== 404) return alert((e as Error).message)
    }
    const all = loadMine()
    delete all[id]
    saveMine(all)
    setMine(all)
    if (!pendingIds.has(id)) {
      setComments((cs) => cs?.filter((c) => c.id !== id) ?? cs)
      setCount(Math.max(0, count - 1 - (comments?.find((c) => c.id === id)?.replies.length ?? 0)))
    }
    setPendingIds((s) => new Set([...s].filter((x) => x !== id)))
  }

  const removeAsAdmin = async (c: GuestComment, parent?: GuestComment) => {
    if (!confirm(parent ? 'この返信を削除します。' : 'このコメントを削除します（返信も一緒に消えます）。')) return
    try {
      await api.deleteComment(c.id)
    } catch (e) {
      return alert((e as Error).message)
    }
    setComments((cs) =>
      parent
        ? cs?.map((x) => (x.id === parent.id ? { ...x, replies: x.replies.filter((r) => r.id !== c.id) } : x)) ?? cs
        : cs?.filter((x) => x.id !== c.id) ?? cs,
    )
    setCount(Math.max(0, count - 1 - (parent ? 0 : c.replies.length)))
  }

  const onReplied = (parent: GuestComment, r: GuestComment) => {
    setComments((cs) => cs?.map((x) => (x.id === parent.id ? { ...x, replies: [...x.replies, r] } : x)) ?? cs)
    setCount(count + 1)
  }

  const onPosted = (m: Mine & { id: number }) => {
    const all = { ...loadMine(), [m.id]: { token: m.token, page_id: m.page_id, name: m.name, body: m.body, created_at: m.created_at } }
    saveMine(all)
    setMine(all)
    setPendingIds((s) => new Set(s).add(m.id))
    setWriting(false)
  }

  const pendingMine = myHere.filter(([id]) => pendingIds.has(Number(id)))

  return (
    <section className="guest-comments">
      <button type="button" className="btn btn-sm btn-link text-decoration-none p-0" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        {open ? '▾' : '▸'} 💬 コメント{count > 0 ? ` ${count}件` : ''}
      </button>
      {open && (
        <div className="guest-comments-body">
          {error && <p className="error small">{error}</p>}
          {comments === null && !error && <p className="muted small">読み込み中…</p>}
          {comments && comments.length === 0 && pendingMine.length === 0 && (
            <p className="muted small mb-2">まだコメントはありません</p>
          )}
          {comments && comments.length > 0 && (
            <ol className="guest-comment-list">
              {comments.map((c) => (
                <li key={c.id} className="guest-comment">
                  <CommentHead c={c}>
                    {mine[c.id] && <DeleteLink onClick={() => removeMine(c.id)} />}
                    {me?.is_admin && <DeleteLink onClick={() => removeAsAdmin(c)} />}
                  </CommentHead>
                  <p className="guest-comment-body">{c.body}</p>
                  {c.replies.length > 0 && (
                    <ol className="guest-comment-replies">
                      {c.replies.map((r) => (
                        <li key={r.id} className="guest-comment">
                          <CommentHead c={r}>{me?.is_admin && <DeleteLink onClick={() => removeAsAdmin(r, c)} />}</CommentHead>
                          <p className="guest-comment-body">{r.body}</p>
                        </li>
                      ))}
                    </ol>
                  )}
                  {me?.is_admin && <AdminReply parent={c} onReplied={(r) => onReplied(c, r)} />}
                </li>
              ))}
            </ol>
          )}
          {pendingMine.length > 0 && (
            <ol className="guest-comment-list">
              {pendingMine.map(([id, m]) => (
                <li key={id} className="guest-comment pending">
                  <header className="guest-comment-head">
                    <span className="timeline-author">{m.name}</span>
                    <span className="muted">{ago(m.created_at)}</span>
                    <span className="badge text-bg-secondary">承認待ち（あなたにだけ表示）</span>
                    <DeleteLink onClick={() => removeMine(Number(id))} />
                  </header>
                  <p className="guest-comment-body">{m.body}</p>
                </li>
              ))}
            </ol>
          )}
          {!me &&
            (writing ? (
              <CommentFormView
                postId={postId}
                captcha={store!.inter.captcha}
                onPosted={onPosted}
                onCancel={() => setWriting(false)}
              />
            ) : (
              <button type="button" className="btn btn-sm btn-outline-secondary guest-comment-write" onClick={() => setWriting(true)}>
                ✏ コメントを書く
              </button>
            ))}
        </div>
      )}
    </section>
  )
}

function CommentHead({ c, children }: { c: GuestComment; children?: ReactNode }) {
  return (
    <header className="guest-comment-head">
      <span className="timeline-author">{c.name}</span>
      {c.is_staff && <span className="badge text-bg-primary">管理人</span>}
      <span className="muted" title={new Date(c.created_at).toLocaleString('ja-JP')}>
        {ago(c.created_at)}
      </span>
      {children}
    </header>
  )
}

function DeleteLink({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="btn btn-sm btn-link text-decoration-none p-0 ms-auto guest-comment-delete" onClick={onClick}>
      削除
    </button>
  )
}

/** 管理者の返信欄（すぐ公開） */
function AdminReply({ parent, onReplied }: { parent: GuestComment; onReplied: (r: GuestComment) => void }) {
  const [open, setOpen] = useState(false)
  const [body, setBody] = useState('')
  const [busy, setBusy] = useState(false)

  const send = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    try {
      onReplied(await api.replyComment(parent.id, body))
      setBody('')
      setOpen(false)
    } catch (err) {
      alert((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  if (!open)
    return (
      <button type="button" className="btn btn-sm btn-link text-decoration-none p-0 guest-comment-reply-open" onClick={() => setOpen(true)}>
        返信する
      </button>
    )
  return (
    <form className="guest-comment-reply vstack gap-2" onSubmit={send}>
      <textarea
        className="form-control form-control-sm"
        rows={2}
        maxLength={BODY_MAX}
        value={body}
        autoFocus
        aria-label={`${parent.name} さんへの返信`}
        onChange={(e) => setBody(e.target.value)}
      />
      <div className="d-flex gap-2">
        <button type="submit" className="btn btn-sm btn-primary" disabled={busy || !body.trim()}>
          返信（すぐ公開）
        </button>
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={() => setOpen(false)}>
          やめる
        </button>
      </div>
    </form>
  )
}

/** ゲストのコメント欄。送ると承認待ちになる */
function CommentFormView({
  postId,
  captcha,
  onPosted,
  onCancel,
}: {
  postId: number
  captcha: boolean
  onPosted: (m: Mine & { id: number }) => void
  onCancel: () => void
}) {
  const [name, setName] = useState(loadName)
  const [body, setBody] = useState('')
  const [answer, setAnswer] = useState('')
  const [website, setWebsite] = useState('')
  const [form, setForm] = useState<CommentForm | null>(null)
  const loadedAt = useRef(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setAnswer('')
    try {
      setForm(await api.commentForm(postId))
      loadedAt.current = Date.now()
    } catch (e) {
      setError((e as Error).message)
    }
  }, [postId])

  useEffect(() => {
    refresh()
  }, [refresh])

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (!form) return
    setBusy(true)
    setError(null)
    try {
      // 開いたまま長く置いていたら、送る前にトークンを取り直す（CAPTCHA なら新しい画像を見てもらう）
      if (Date.now() - loadedAt.current > FORM_REFRESH_MS) {
        await refresh()
        if (captcha) {
          setError('時間がたったので画像を新しくしました。新しい画像の文字を入力してください')
          return
        }
      }
      const r = await api.postComment(postId, { name, body, token: form.token, captcha: answer, website })
      saveName(name.trim())
      // 送ったらフォームを閉じる（承認待ちとして一覧の下に出る）
      onPosted({ id: r.id, token: r.delete_token, page_id: postId, name: r.name, body: r.body, created_at: r.created_at })
    } catch (err) {
      setError((err as Error).message)
      // トークンは使い捨てなので、失敗したら新しくする（入力した名前・本文は残す）
      refresh()
    } finally {
      setBusy(false)
    }
  }

  return (
    <form className="guest-comment-form vstack gap-2" onSubmit={submit}>
      <input
        className="form-control form-control-sm"
        placeholder="名前"
        aria-label="名前"
        maxLength={NAME_MAX}
        value={name}
        autoComplete="nickname"
        onChange={(e) => setName(e.target.value)}
      />
      <textarea
        className="form-control form-control-sm"
        placeholder="コメント"
        aria-label="コメント"
        rows={3}
        maxLength={BODY_MAX}
        value={body}
        autoFocus
        onChange={(e) => setBody(e.target.value)}
      />
      {/* 画面には出さない入力欄（機械による送信の目印） */}
      <div className="guest-comment-hp" aria-hidden="true">
        <label>
          Website
          <input tabIndex={-1} autoComplete="off" value={website} onChange={(e) => setWebsite(e.target.value)} />
        </label>
      </div>
      {captcha && (
        <div className="guest-comment-captcha">
          {form?.image ? <img src={form.image} alt="確認用の文字の画像" width={160} height={60} /> : <span className="captcha-placeholder" />}
          <button type="button" className="btn btn-sm btn-link text-decoration-none p-0" onClick={refresh}>
            別の画像
          </button>
          <input
            className="form-control form-control-sm"
            placeholder="画像の文字（大文字・小文字は区別しません）"
            aria-label="画像の文字"
            maxLength={8}
            autoComplete="off"
            autoCapitalize="characters"
            spellCheck={false}
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
          />
        </div>
      )}
      <div className="d-flex flex-wrap align-items-center gap-2">
        <button
          type="submit"
          className="btn btn-sm btn-primary"
          disabled={busy || !form || !name.trim() || !body.trim() || (captcha && !answer.trim())}
        >
          送信
        </button>
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={onCancel}>
          やめる
        </button>
        <span className="muted small">
          コメントは管理者が承認してから公開されます{body.length > BODY_MAX * 0.8 && `（${body.length}/${BODY_MAX}）`}
        </span>
      </div>
      {error && <p className="error small mb-0">{error}</p>}
    </form>
  )
}
