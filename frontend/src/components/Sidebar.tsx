import { useEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { accessBadge, restrictingAncestors } from '../access'
import { api, type PageDetail, type PageSummary } from '../api'
import { childrenMap, displayTitle, useApp } from '../AppContext'
import { navigate, pagePath, useRoute } from '../router'
import TemplatePicker from './TemplatePicker'
import TocList from './TocList'

const EXPANDED_KEY = 'brikrs.expanded'

function loadExpanded(): Set<number> {
  try {
    return new Set(JSON.parse(localStorage.getItem(EXPANDED_KEY) ?? '[]'))
  } catch {
    return new Set()
  }
}

type DropPos = 'before' | 'inside' | 'after'
type DropTarget = { id: number; pos: DropPos }

/** ページツリー。広い画面では左の列、狭い画面ではオフキャンバス（左から出るパネル）になる */
export default function Sidebar({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { editing, pages, setPages, reloadPages, toc } = useApp()
  const route = useRoute()
  const currentId = route.name === 'page' ? route.id : null
  const tree = useMemo(() => childrenMap(pages), [pages])
  const [expanded, setExpanded] = useState<Set<number>>(loadExpanded)
  const [dragId, setDragId] = useState<number | null>(null)
  const [drop, setDrop] = useState<DropTarget | null>(null)
  const [addMenuOpen, setAddMenuOpen] = useState(false)
  const addMenuRef = useRef<HTMLDivElement>(null)
  const [templatePickerOpen, setTemplatePickerOpen] = useState(false)

  useEffect(() => {
    if (!addMenuOpen) return
    const onDown = (e: MouseEvent) => addMenuRef.current && !addMenuRef.current.contains(e.target as Node) && setAddMenuOpen(false)
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [addMenuOpen])

  useEffect(() => {
    try {
      localStorage.setItem(EXPANDED_KEY, JSON.stringify([...expanded]))
    } catch {
      /* 保存できなくても動作には影響しない */
    }
  }, [expanded])

  // 開いているページの祖先は自動で展開する
  useEffect(() => {
    if (currentId === null) return
    const byId = new Map(pages.map((p) => [p.id, p]))
    const toOpen: number[] = []
    let cur = byId.get(currentId)
    while (cur?.parent_id != null) {
      toOpen.push(cur.parent_id)
      cur = byId.get(cur.parent_id)
    }
    // expanded を依存に入れると手動で閉じられなくなるので、関数形式で更新する
    setExpanded((s) => (toOpen.every((id) => s.has(id)) ? s : new Set([...s, ...toOpen])))
  }, [currentId, pages])

  const toggle = (id: number) =>
    setExpanded((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })

  const addPage = async (parentId: number | null) => {
    const p = await api.createPage(parentId)
    await reloadPages()
    if (parentId !== null) setExpanded((s) => new Set([...s, parentId]))
    navigate(pagePath(p.id))
  }

  const addFromTemplate = async (tpl: PageDetail) => {
    setTemplatePickerOpen(false)
    const p = await api.createFromTemplate(null, tpl)
    await reloadPages()
    navigate(pagePath(p.id))
  }

  const isDescendant = (id: number, maybeAncestor: number) => {
    const byId = new Map(pages.map((p) => [p.id, p]))
    let cur = byId.get(id)
    while (cur) {
      if (cur.id === maybeAncestor) return true
      cur = cur.parent_id === null ? undefined : byId.get(cur.parent_id)
    }
    return false
  }

  const onDragOver = (e: DragEvent, target: PageSummary) => {
    if (dragId === null || isDescendant(target.id, dragId)) return
    e.preventDefault()
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const y = (e.clientY - rect.top) / rect.height
    const pos: DropPos = y < 0.25 ? 'before' : y > 0.75 ? 'after' : 'inside'
    if (drop?.id !== target.id || drop.pos !== pos) setDrop({ id: target.id, pos })
  }

  const onDrop = async (e: DragEvent) => {
    e.preventDefault()
    const d = drop
    const id = dragId
    setDrop(null)
    setDragId(null)
    if (!d || id === null || d.id === id) return
    const target = pages.find((p) => p.id === d.id)
    if (!target) return
    let parentId: number | null
    let index: number | undefined
    if (d.pos === 'inside') {
      parentId = target.id
      index = undefined
      setExpanded((s) => new Set([...s, target.id]))
    } else {
      parentId = target.parent_id
      const siblings = (tree.get(parentId) ?? []).filter((p) => p.id !== id)
      index = siblings.findIndex((p) => p.id === target.id) + (d.pos === 'after' ? 1 : 0)
    }
    // 親ページの下書き・公開範囲の制限が変わる移動は、見られる人が変わるので確かめる
    const moved = pages.find((p) => p.id === id)
    if (moved && moved.parent_id !== parentId) {
      const before = restrictingAncestors(pages, moved.parent_id)
      const after = restrictingAncestors(pages, parentId)
      const changed = before.size !== after.size || [...before].some((x) => !after.has(x))
      if (changed) {
        const wider = [...before].some((x) => !after.has(x))
        const msg = wider
          ? `「${displayTitle(moved.title)}」（子ページも）を、親ページの下書き・公開範囲の制限の外へ移します。今より多くの人に見えるようになることがあります。`
          : `「${displayTitle(moved.title)}」（子ページも）を、下書き・公開範囲を限ったページの下へ移します。今まで見られた人（作った人も）が見られなくなることがあります。`
        if (!confirm(`${msg}\n移しますか？（できるのは、このページを作った人と管理者だけです）`)) return
      }
    }
    try {
      setPages(await api.movePage(id, parentId, index))
    } catch (err) {
      alert((err as Error).message)
    }
  }

  const renderNodes = (parentId: number | null, depth: number) =>
    (tree.get(parentId) ?? []).map((p) => {
      // タイムラインの投稿・データベースの行（子ページ）は数が多くなるので、ツリーには出さない
      const timeline = p.layout === 'timeline'
      const database = p.layout === 'database'
      const kids = timeline || database ? [] : (tree.get(p.id) ?? [])
      const open = expanded.has(p.id) && !timeline && !database
      const dropCls = drop?.id === p.id ? `drop-${drop.pos}` : ''
      return (
        <div key={p.id}>
          <div
            className={`tree-row ${p.id === currentId ? 'active' : ''} ${dropCls}`}
            style={{ paddingLeft: 8 + depth * 14 }}
            // ロック中のページは移動できない（子ページは動かせる）
            draggable={editing && !p.is_locked}
            onDragStart={(e) => {
              setDragId(p.id)
              e.dataTransfer.effectAllowed = 'move'
            }}
            onDragEnd={() => {
              setDragId(null)
              setDrop(null)
            }}
            onDragOver={(e) => onDragOver(e, p)}
            onDragLeave={() => drop?.id === p.id && setDrop(null)}
            onDrop={onDrop}
            onClick={() => navigate(pagePath(p.id))}
          >
            <button
              className={`tree-toggle ${kids.length ? '' : 'empty'}`}
              disabled={timeline || database}
              onClick={(e) => {
                e.stopPropagation()
                toggle(p.id)
              }}
              aria-label={open ? '閉じる' : '開く'}
            >
              {open ? '▾' : '▸'}
            </button>
            <span className="tree-icon">{p.icon || '📄'}</span>
            <span className={`tree-title${p.is_draft ? ' draft' : ''}`}>{displayTitle(p.title)}</span>
            {timeline && (
              <span className="tree-badge" title="タイムライン（投稿はページを開くと表示）">
                {(tree.get(p.id) ?? []).length}
              </span>
            )}
            {database && (
              <span className="tree-badge" title="データベース（行はページを開くと表示）">
                🗃 {(tree.get(p.id) ?? []).length}
              </span>
            )}
            {p.is_locked && (
              <span className="tree-badge" title="ロック中（編集できるのは作った人と管理者だけ。移動・ゴミ箱は外してから）">
                🔐
              </span>
            )}
            {p.is_draft && <span className="tree-badge">下書き</span>}
            {!p.is_draft && accessBadge(p) && (
              <span className="tree-badge" title="公開範囲を限っています（子ページも含む）">
                🔒 {accessBadge(p)}
              </span>
            )}
            {editing && (
              <button
                className="tree-add"
                title="子ページを追加"
                onClick={(e) => {
                  e.stopPropagation()
                  addPage(p.id)
                }}
              >
                ＋
              </button>
            )}
          </div>
          {open && kids.length > 0 && renderNodes(p.id, depth + 1)}
          {open && kids.length === 0 && (
            <div className="tree-empty" style={{ paddingLeft: 30 + depth * 14 }}>
              子ページなし
            </div>
          )}
        </div>
      )
    })

  return (
    <>
      <aside className={`offcanvas-lg offcanvas-start app-sidebar${open ? ' show' : ''}`} aria-label="ページ一覧">
        <div className="offcanvas-header">
          <h5 className="offcanvas-title brand-font">ページ一覧</h5>
          <button type="button" className="btn-close" onClick={onClose} aria-label="閉じる" />
        </div>
        <div className="offcanvas-body">
          <div className="card side-card shadow-sm">
            <div className="card-header text-center brand-font">ページ</div>
            <nav className="tree">
              {pages.length ? renderNodes(null, 0) : <div className="tree-empty px-3">まだページがありません</div>}
            </nav>
            {editing && (
              <div className="card-footer p-1 dropdown" ref={addMenuRef}>
                <button
                  type="button"
                  className="btn btn-sm btn-link w-100 text-start text-decoration-none"
                  aria-expanded={addMenuOpen}
                  onClick={() => setAddMenuOpen((o) => !o)}
                >
                  ＋ 新しいページ
                </button>
                {/* サイドバーはスクロール領域なので、浮かせると下に目次カードが無いときに切れる。フッターの中に展開する */}
                <ul className={`dropdown-menu side-add-menu${addMenuOpen ? ' show' : ''}`}>
                  <li>
                    <button
                      className="dropdown-item"
                      onClick={() => {
                        setAddMenuOpen(false)
                        addPage(null)
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
            )}
          </div>
          {toc.length > 0 && (
            <div className="card side-card shadow-sm mt-3">
              <div className="card-header text-center brand-font">目次</div>
              <div className="card-body p-2">
                <TocList items={toc} onPick={onClose} />
              </div>
            </div>
          )}
        </div>
      </aside>
      {open && <div className="offcanvas-backdrop fade show d-lg-none" onClick={onClose} />}
      {templatePickerOpen && (
        <TemplatePicker onPick={addFromTemplate} onClose={() => setTemplatePickerOpen(false)} />
      )}
    </>
  )
}
