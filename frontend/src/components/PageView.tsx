import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { api, ApiError, type PageDetail, type PageSummary, type ThreadDetail } from '../api'
import { ancestors, childrenMap, displayTitle, useApp } from '../AppContext'
import { navigate, pagePath } from '../router'
import BlockView from './BlockView'
import DatabaseTable from './DatabaseTable'
import HistoryDialog from './HistoryDialog'
import IconPicker from './IconPicker'
import { InteractionsContext, useInteractionsStore } from '../interactions'
import { GuestComments, ReactionBar } from './Interactions'
import PageMenu, { type PageSettings } from './PageMenu'
import AccessDialog from './AccessDialog'
import { accessLabel, canManage, lockedForMe, useGroups } from '../access'
import Timeline from './Timeline'
import Thread, { PostHead } from './Thread'
import TemplatePicker from './TemplatePicker'
import { headingsOf, TOC_MIN_HEADINGS, type TocItem } from '../toc'
import { registerFlush } from '../saves'
import TocList from './TocList'

// BlockNote は大きいので、編集できる人にだけ後から読み込む（閲覧は BlockView で描画）
const Editor = lazy(() => import('./Editor'))

type SaveState = 'saved' | 'pending' | 'saving' | 'error' | 'conflict'
type Patch = { title?: string; icon?: string; content?: unknown[] }

const SAVE_DELAY_MS = 800

export default function PageView({ id }: { id: number }) {
  const { me, editing, setEditing, pages, patchPage, reloadPages, setToc } = useApp()
  // 目次の元になる本文（保存のたびに更新する。打鍵ごとには作り直さない）
  const [tocBlocks, setTocBlocks] = useState<unknown[]>([])
  const [page, setPage] = useState<PageDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [title, setTitle] = useState('')
  const [icon, setIcon] = useState('')
  const [saveState, setSaveState] = useState<SaveState>('saved')
  const [pickerOpen, setPickerOpen] = useState(false)
  const [historyOpen, setHistoryOpen] = useState(false)
  const [addMenuOpen, setAddMenuOpen] = useState(false)
  const addMenuRef = useRef<HTMLDivElement>(null)
  const [templatePickerOpen, setTemplatePickerOpen] = useState(false)
  const [accessOpen, setAccessOpen] = useState(false)
  const groups = useGroups(me)
  /** 保存しようとしたら他の人が先に更新していた。そのときの最新のページ */
  const [conflict, setConflict] = useState<PageDetail | null>(null)
  const [backlinks, setBacklinks] = useState<number[]>([])
  /** タイムラインの投稿・返信なら、そのスレッド（元の投稿までの流れと、このページへの返信） */
  const [thread, setThread] = useState<ThreadDetail | null>(null)
  const threadTop = useRef<HTMLDivElement>(null)
  // 版を復元したときに BlockNote を作り直すためのキー
  const [editorKey, setEditorKey] = useState(0)
  // 編集モードのときだけ編集できる（閲覧モードはログインしていないときと同じ見た目。下書きは見える）。
  // ロック中のページは、作った人と管理者のほかは閲覧だけ
  const locked = !!page && lockedForMe(me, page)
  const editable = editing && !locked
  /** エディタの最新の本文（閲覧モードへ戻したとき、BlockView にこれを出す） */
  const latestContent = useRef<unknown[] | null>(null)

  // ---- 自動保存：変更をまとめて一定時間後に送る ----
  const pending = useRef<Patch>({})
  const timer = useRef<number | undefined>(undefined)
  /** 手元の内容がどの版を元にしているか。保存時に送り、違っていればサーバーが 409 を返す */
  const version = useRef(0)
  // 保存は1つずつ順に送る（並行して送ると、自分の保存同士が衝突と判定される）
  const queue = useRef<Promise<void>>(Promise.resolve())
  const inConflict = useRef(false)

  /** サーバーの最新の内容で画面を置き換える（手元の未保存の変更は捨てる） */
  const applyServerPage = useCallback(
    (p: PageDetail) => {
      version.current = p.version
      setTocBlocks(p.content)
      setPage(p)
      setTitle(p.title)
      setIcon(p.icon)
      patchPage(p.id, { title: p.title, icon: p.icon })
      document.title = displayTitle(p.title)
      latestContent.current = null
      setEditorKey((k) => k + 1)
    },
    [patchPage],
  )

  const save = useCallback(async () => {
    const patch = pending.current
    if (Object.keys(patch).length === 0 || inConflict.current) return
    pending.current = {}
    setSaveState('saving')
    try {
      const updated = await api.updatePage(id, { ...patch, base_version: version.current })
      if (patch.content) setTocBlocks(patch.content)
      version.current = updated.version
      setPage((p) => (p ? { ...p, updated_at: updated.updated_at, updated_by: updated.updated_by } : p))
      // 保存前にページ一覧が再読み込みされると、ツリー側が古いタイトルに戻ってしまうので、
      // 保存結果で揃え直す（さらに新しい未保存の変更がある項目は除く）
      const fix: { title?: string; icon?: string } = {}
      if (!('title' in pending.current)) fix.title = updated.title
      if (!('icon' in pending.current)) fix.icon = updated.icon
      patchPage(id, fix)
      setSaveState(Object.keys(pending.current).length ? 'pending' : 'saved')
    } catch (e) {
      // 失敗した分は次の保存で再送する
      pending.current = { ...patch, ...pending.current }
      if (e instanceof ApiError && e.status === 409) {
        inConflict.current = true
        setConflict(e.data.latest as PageDetail)
        setSaveState('conflict')
      } else {
        setSaveState('error')
        console.error(e)
      }
    }
  }, [id, patchPage])

  const flush = useCallback(() => {
    window.clearTimeout(timer.current)
    queue.current = queue.current.then(save)
    return queue.current
  }, [save])

  const schedule = useCallback(
    (patch: Patch) => {
      if (patch.content) latestContent.current = patch.content
      pending.current = { ...pending.current, ...patch }
      setSaveState('pending')
      window.clearTimeout(timer.current)
      timer.current = window.setTimeout(flush, SAVE_DELAY_MS)
    },
    [flush],
  )

  // 閲覧モードへ切り替えたら、未保存分をすぐ送り、編集した本文を閲覧表示へ反映する
  useEffect(() => {
    if (editing) return
    setPickerOpen(false)
    setAddMenuOpen(false)
    flush()
    const c = latestContent.current
    if (c) {
      latestContent.current = null
      setTocBlocks(c)
      setPage((p) => (p ? { ...p, content: c } : p))
    }
  }, [editing, flush])

  // ログアウトの前に、ナビバーから未保存分を送れるようにする
  useEffect(() => registerFlush(flush), [flush])

  // ページを離れるとき・タブを閉じるときに未保存分を送る
  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (Object.keys(pending.current).length) {
        flush()
        e.preventDefault()
      }
    }
    window.addEventListener('beforeunload', onBeforeUnload)
    return () => {
      window.removeEventListener('beforeunload', onBeforeUnload)
      flush()
    }
  }, [flush])

  useEffect(() => {
    api
      .getPage(id)
      .then((p) => {
        version.current = p.version
        setTocBlocks(p.content)
        setPage(p)
        setTitle(p.title)
        setIcon(p.icon)
        document.title = displayTitle(p.title)
      })
      .catch((e) => setError(e instanceof ApiError && e.status === 404 ? 'ページが見つかりません' : e.message))
  }, [id])

  const loadBacklinks = useCallback(
    () =>
      api
        .backlinks(id)
        .then((ps) => setBacklinks(ps.map((p) => p.id)))
        .catch(() => setBacklinks([])),
    [id],
  )
  useEffect(() => {
    loadBacklinks()
  }, [loadBacklinks])

  // タブに戻ってきたとき、その間に他の人が更新していれば読み直す（未保存の変更があるときは保存時の衝突検知に任せる）
  useEffect(() => {
    const onVisible = async () => {
      if (document.visibilityState !== 'visible' || version.current === 0) return
      await queue.current
      const idle = () => Object.keys(pending.current).length === 0 && !inConflict.current
      if (!idle()) return
      const latest = await api.getPage(id).catch(() => null)
      if (latest && latest.version > version.current && idle()) applyServerPage(latest)
      loadBacklinks()
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => document.removeEventListener('visibilitychange', onVisible)
  }, [id, applyServerPage, loadBacklinks])

  // 祖先にタイムラインがあれば、このページはその投稿・返信
  const inThread = ancestors(pages, id)
    .slice(0, -1)
    .some((c) => c.layout === 'timeline')
  useEffect(() => {
    setThread(null)
    if (!inThread) return
    let alive = true
    api
      .thread(id)
      .then((t) => alive && setThread(t))
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [id, inThread])
  // 返信を開いたときは、元の投稿からの流れを上に出したうえで、このページの位置までスクロールする
  const hasAncestors = !!thread?.ancestors.length
  useEffect(() => {
    if (hasAncestors) threadTop.current?.scrollIntoView({ block: 'start' })
  }, [hasAncestors])

  const isTimeline = page?.layout === 'timeline'
  const toc: TocItem[] = useMemo(() => {
    const hs = isTimeline ? [] : headingsOf(tocBlocks)
    return hs.length >= TOC_MIN_HEADINGS ? hs : []
  }, [tocBlocks, isTimeline])
  // サイドバーに目次を出す（ページを離れたら消す）
  useEffect(() => {
    setToc(toc)
  }, [toc, setToc])
  useEffect(() => () => setToc([]), [setToc])

  useEffect(() => {
    if (!addMenuOpen) return
    const onDown = (e: MouseEvent) => addMenuRef.current && !addMenuRef.current.contains(e.target as Node) && setAddMenuOpen(false)
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [addMenuOpen])

  if (error) return <div className="message">{error}</div>
  if (!page) return <div className="loading">読み込み中…</div>

  const crumbs = ancestors(pages, id)
  const children = childrenMap(pages).get(id) ?? []

  const changeTitle = (t: string) => {
    setTitle(t)
    patchPage(id, { title: t })
    document.title = displayTitle(t)
    schedule({ title: t })
  }

  const changeIcon = (i: string) => {
    setIcon(i)
    patchPage(id, { icon: i })
    schedule({ icon: i })
    setPickerOpen(false)
  }

  const remove = async () => {
    const n = countDescendants(id)
    const msg = n
      ? `「${displayTitle(title)}」と子ページ ${n} 件をゴミ箱へ移します。`
      : `「${displayTitle(title)}」をゴミ箱へ移します。`
    if (!confirm(`${msg}（ゴミ箱から復元できます）`)) return
    try {
      await flush()
      await api.deletePage(id)
    } catch (e) {
      // 子ページにロック中のページがあるときなど
      alert((e as Error).message)
      return
    }
    await reloadPages()
    navigate(page.parent_id ? pagePath(page.parent_id) : '/', true)
  }

  const countDescendants = (rootId: number): number => {
    const map = childrenMap(pages)
    const kids = map.get(rootId) ?? []
    return kids.reduce((n, k) => n + 1 + countDescendants(k.id), 0)
  }

  const openHistory = async () => {
    // 未保存の編集を先に履歴へ反映させる
    await flush()
    setHistoryOpen(true)
  }

  /** 衝突したとき：自分の未保存の変更で上書きする（相手の版は履歴に残っている） */
  const overwrite = () => {
    if (!conflict) return
    version.current = conflict.version
    inConflict.current = false
    setConflict(null)
    flush()
  }

  /** 衝突したとき：自分の未保存の変更を捨てて、最新の内容を読み込む */
  const discardMine = () => {
    if (!conflict) return
    pending.current = {}
    inConflict.current = false
    setConflict(null)
    setSaveState('saved')
    applyServerPage(conflict)
  }

  /** 設定の変更を画面とツリーへ反映する（本文は触らない） */
  const applySettings = (p: PageDetail) => {
    setPage((cur) =>
      cur
        ? {
            ...cur,
            is_draft: p.is_draft,
            visibility: p.visibility,
            group_ids: p.group_ids,
            layout: p.layout,
            is_template: p.is_template,
            allow_guest_comments: p.allow_guest_comments,
            allow_reactions: p.allow_reactions,
            is_locked: p.is_locked,
          }
        : cur,
    )
    patchPage(id, {
      is_draft: p.is_draft,
      visibility: p.visibility,
      layout: p.layout,
      is_template: p.is_template,
      is_locked: p.is_locked,
    })
  }

  /** 表示形式、テンプレート、ゲストコメント・リアクションの受け付けの切り替え */
  const changeSettings = async (s: PageSettings) => {
    try {
      applySettings(await api.updateSettings(id, s))
    } catch (e) {
      alert((e as Error).message)
    }
  }

  // 祖先が下書き・公開範囲を限っていれば、このページも同じく限られる（一番近い祖先を出す）
  const restrictedAncestor = crumbs
    .slice(0, -1)
    .reverse()
    .find((c) => c.is_draft || c.visibility !== 'public')
  const manageAccess = canManage(me, page.created_by_id)
  const ownAccessLabel = page.is_draft ? '下書き' : accessLabel(page, groups)
  const timeline = page.layout === 'timeline'
  const database = page.layout === 'database'

  const addChild = async () => {
    const p = await api.createPage(id)
    await reloadPages()
    navigate(pagePath(p.id))
  }

  const addChildFromTemplate = async (tpl: PageDetail) => {
    setTemplatePickerOpen(false)
    const p = await api.createFromTemplate(id, tpl)
    await reloadPages()
    navigate(pagePath(p.id))
  }

  return (
    <article className={`page${editable ? ' editable' : ''}`}>
      <header className="page-topbar">
        <nav className="breadcrumbs">
          {crumbs.map((c, i) => (
            <span key={c.id}>
              {i > 0 && <span className="sep">/</span>}
              <a href={pagePath(c.id)} onClick={(e) => (e.preventDefault(), navigate(pagePath(c.id)))}>
                {c.icon && <span className="crumb-icon">{c.icon}</span>}
                {displayTitle(c.title)}
              </a>
            </span>
          ))}
        </nav>
        {me && (
          <div className="page-actions">
            {editing && (
              <>
                {editable && (
                  <span className={`save-state ${saveState}`}>
                    {
                      {
                        saved: '保存済み',
                        pending: '編集中…',
                        saving: '保存中…',
                        error: '保存失敗（再試行します）',
                        conflict: '未保存（衝突）',
                      }[saveState]
                    }
                  </span>
                )}
                <div className="dropdown" ref={addMenuRef}>
                  <button
                    className="btn btn-sm btn-outline-secondary"
                    aria-expanded={addMenuOpen}
                    onClick={() => setAddMenuOpen((o) => !o)}
                  >
                    ＋<span className="d-none d-sm-inline"> 子ページ</span>
                  </button>
                  <ul className={`dropdown-menu shadow${addMenuOpen ? ' show' : ''}`} data-bs-popper="static">
                    <li>
                      <button
                        className="dropdown-item"
                        onClick={() => {
                          setAddMenuOpen(false)
                          addChild()
                        }}
                      >
                        空のページ
                      </button>
                    </li>
                    <li>
                      <button
                        className="dropdown-item"
                        onClick={() => {
                          setAddMenuOpen(false)
                          setTemplatePickerOpen(true)
                        }}
                      >
                        テンプレートから作成…
                      </button>
                    </li>
                  </ul>
                </div>
                <button className="btn btn-sm btn-outline-secondary" onClick={openHistory}>
                  履歴
                </button>
                <PageMenu
                  canManageAccess={manageAccess}
                  accessSummary={ownAccessLabel}
                  onAccess={() => setAccessOpen(true)}
                  layout={page.layout}
                  isTemplate={page.is_template}
                  allowGuestComments={page.allow_guest_comments}
                  allowReactions={page.allow_reactions}
                  isLocked={page.is_locked}
                  canEdit={!locked}
                  onSettings={changeSettings}
                  onTrash={remove}
                />
              </>
            )}
            {/* 編集モードでは変更が自動で保存され続けるので、「完了」ではなく表示の切り替えにしている */}
            <div className="btn-group btn-group-sm mode-toggle ms-1" role="group" aria-label="表示モード">
              <button
                type="button"
                className={`btn ${editable ? 'btn-outline-secondary' : 'btn-secondary'}`}
                aria-pressed={!editable}
                onClick={() => setEditing(false)}
              >
                閲覧
              </button>
              <button
                type="button"
                className={`btn ${editable ? 'btn-primary' : 'btn-outline-primary'}`}
                aria-pressed={editable}
                onClick={() => setEditing(true)}
              >
                編集
              </button>
            </div>
          </div>
        )}
      </header>

      {conflict && (
        <div className="alert alert-warning shadow conflict-banner" role="alert">
          <p>
            <strong>{conflict.updated_by ?? '他の人'}</strong> が{' '}
            {new Date(conflict.updated_at).toLocaleString('ja-JP')}{' '}
            にこのページを更新したため、あなたの変更はまだ保存されていません。
          </p>
          <div className="conflict-actions">
            <button className="btn btn-sm btn-primary" onClick={overwrite}>
              自分の変更で上書き
            </button>
            <button className="btn btn-sm btn-outline-secondary" onClick={discardMine}>
              自分の変更を捨てて最新を読み込む
            </button>
          </div>
          <p className="muted">どちらを選んでも、相手の版は「履歴」から確認・復元できます。</p>
        </div>
      )}

      <div className="page-body">
        {me && page.is_locked && (
          <div className="alert alert-secondary py-2 small draft-notice">
            <strong>🔐 ロック中</strong>：
            {locked
              ? '編集できるのは、ページを作った人と管理者だけです（子ページの追加や返信はできます）。'
              : '作った人と管理者のほかは閲覧だけです。移動・ゴミ箱へ移すには、「…」メニューからロックを外してください。'}
          </div>
        )}
        {me && (page.is_draft || page.visibility !== 'public' || restrictedAncestor) && (
          <div className="alert alert-secondary py-2 small draft-notice">
            {page.is_draft ? (
              <>
                <strong>下書き</strong>：作った人と管理者にだけ表示されます（子ページも含む）。
              </>
            ) : page.visibility !== 'public' ? (
              <>
                <strong>公開範囲</strong>：{accessLabel(page, groups)}に表示されます（子ページも含む）。
              </>
            ) : null}
            {restrictedAncestor && (
              <>
                {' '}
                親ページ「{displayTitle(restrictedAncestor.title)}」が
                {restrictedAncestor.is_draft
                  ? '下書き'
                  : restrictedAncestor.visibility === 'members'
                    ? 'ログインしている人だけに公開'
                    : 'グループを限って公開'}
                のため、このページもその範囲に限られます。
              </>
            )}
            {editable && manageAccess && (
              <button className="btn btn-link btn-sm p-0 ms-2 align-baseline" onClick={() => setAccessOpen(true)}>
                公開範囲を変える
              </button>
            )}
          </div>
        )}
        {thread && (
          <nav className="thread-context">
            <a
              href={pagePath(thread.timeline.id)}
              onClick={(e) => (e.preventDefault(), navigate(pagePath(thread.timeline.id)))}
            >
              ← {thread.timeline.icon && `${thread.timeline.icon} `}
              {displayTitle(thread.timeline.title)}へ
            </a>
            {thread.ancestors.map((a) => (
              <div key={a.id} className="thread-ancestor">
                <PostHead p={a} />
                <BlockView blocks={a.content} />
              </div>
            ))}
            {thread.ancestors.length > 0 && <div ref={threadTop} className="thread-here muted small">↓ この返信</div>}
          </nav>
        )}
        <div className="page-icon-wrap">
          {icon ? (
            <button className="page-icon" disabled={!editable} onClick={() => setPickerOpen((o) => !o)}>
              {icon}
            </button>
          ) : (
            editable && (
              <button className="ghost add-icon" onClick={() => setPickerOpen((o) => !o)}>
                ☺ アイコンを追加
              </button>
            )
          )}
          {pickerOpen && <IconPicker onPick={changeIcon} onClose={() => setPickerOpen(false)} hasIcon={!!icon} />}
        </div>

        {editable ? (
          <input
            className="page-title"
            value={title}
            placeholder="無題"
            maxLength={255}
            onChange={(e) => changeTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                document.querySelector<HTMLElement>('.bn-editor')?.focus()
              }
            }}
          />
        ) : (
          <h1 className="page-title">{displayTitle(title)}</h1>
        )}

        <div className="page-meta">
          最終更新 {new Date(page.updated_at).toLocaleString('ja-JP')}
          {page.updated_by && ` · ${page.updated_by}`}
        </div>

        {toc.length > 0 && (
          <details className="toc-inline d-lg-none">
            <summary className="brand-font">目次</summary>
            <TocList items={toc} />
          </details>
        )}

        <div className="editor-wrap">
          {editable ? (
            <Suspense fallback={<BlockView blocks={page.content} />}>
              <Editor
                key={editorKey}
                pageId={id}
                initialContent={page.content}
                editable
                onChange={(blocks) => schedule({ content: blocks })}
              />
            </Suspense>
          ) : (
            <BlockView key={editorKey} blocks={page.content} />
          )}
        </div>

        {thread && (
          // 下の返信の ThreadInteractions と同じ親の中に並ぶので、key を分ける（同じだと React が要素を取り違え、
          // 閲覧・編集を切り替えるたびにリアクションの欄が増えていた）
          <ThreadInteractions key={`reactions-${id}`} id={id} thread={thread}>
            <ReactionBar id={id} />
            {thread.ancestors.length === 0 && !page.is_draft && page.visibility === 'public' && !restrictedAncestor && (
              <GuestComments postId={id} openIfAny />
            )}
          </ThreadInteractions>
        )}

        {timeline ? (
          // 受け付けを切り替えたら読み込み直す（件数・受け付けは一覧の応答に入っている）
          <Timeline key={`${id}-${page.allow_guest_comments}-${page.allow_reactions}`} pageId={id} />
        ) : database ? (
          <DatabaseTable
            key={id}
            pageId={id}
            initialSchema={page.db_schema ?? []}
            editable={editing}
            schemaEditable={editable}
          />
        ) : inThread ? (
          thread && (
            <ThreadInteractions key={`replies-${id}`} id={id} thread={thread}>
              <section className="thread-page">
                <h2>返信</h2>
                <Thread key={id} id={id} replies={thread.replies} alwaysOpen />
                {thread.replies.length === 0 && !editable && <p className="muted small">まだ返信はありません</p>}
              </section>
            </ThreadInteractions>
          )
        ) : (
          <PageList title="子ページ" items={children} />
        )}

        <PageList title="このページへのリンク" items={backlinks.flatMap((b) => pages.filter((p) => p.id === b))} />
      </div>
      {accessOpen && <AccessDialog page={page} onSaved={applySettings} onClose={() => setAccessOpen(false)} />}
      {historyOpen && (
        <HistoryDialog
          pageId={id}
          onClose={() => setHistoryOpen(false)}
          onRestored={applyServerPage}
          canRestore={!locked}
        />
      )}
      {templatePickerOpen && (
        <TemplatePicker onPick={addChildFromTemplate} onClose={() => setTemplatePickerOpen(false)} />
      )}
    </article>
  )
}

/** スレッド表示（投稿・返信のページ）のリアクション・コメントの件数を配る */
function ThreadInteractions({ thread, children }: { id: number; thread: ThreadDetail; children: ReactNode }) {
  const store = useInteractionsStore(thread.interactions)
  return <InteractionsContext.Provider value={store}>{children}</InteractionsContext.Provider>
}

function PageList({ title, items }: { title: string; items: PageSummary[] }) {
  if (items.length === 0) return null
  return (
    <section className="child-pages">
      <h2>{title}</h2>
      <ul>
        {items.map((c) => (
          <li key={c.id}>
            <a href={pagePath(c.id)} onClick={(e) => (e.preventDefault(), navigate(pagePath(c.id)))}>
              <span className="tree-icon">{c.icon || '📄'}</span>
              {displayTitle(c.title)}
            </a>
          </li>
        ))}
      </ul>
    </section>
  )
}

