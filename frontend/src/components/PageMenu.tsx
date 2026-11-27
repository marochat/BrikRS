import { useEffect, useRef, useState } from 'react'
import type { PageLayout } from '../api'
import { useApp } from '../AppContext'

export type PageSettings = {
  layout?: PageLayout
  is_template?: boolean
  allow_guest_comments?: boolean
  allow_reactions?: boolean
  is_locked?: boolean
}

type Props = {
  /** 下書き・公開範囲を変えられるか（作った人と管理者） */
  canManageAccess: boolean
  /** 今の公開範囲の説明（メニューに小さく出す） */
  accessSummary: string
  onAccess: () => void
  layout: PageLayout
  isTemplate: boolean
  allowGuestComments: boolean
  allowReactions: boolean
  /** ロック中（ゴミ箱へ移せない。外せるのは canManageAccess の人） */
  isLocked: boolean
  /** false なら表示形式などの設定も変えられない（ロック中で、作った人・管理者でないとき） */
  canEdit: boolean
  onSettings: (s: PageSettings) => void
  onTrash: () => void
}

const LAYOUT_LABELS: Record<PageLayout, string> = {
  page: '通常のページ',
  timeline: 'タイムライン',
  database: 'データベース',
}

/** ページの「…」メニュー：公開範囲・下書き、ロック、表示形式、（タイムラインなら）ゲストコメント・リアクション、テンプレート、ゴミ箱 */
export default function PageMenu({
  canManageAccess,
  accessSummary,
  onAccess,
  layout,
  isTemplate,
  allowGuestComments,
  allowReactions,
  isLocked,
  canEdit,
  onSettings,
  onTrash,
}: Props) {
  const { site } = useApp()
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false)
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    document.addEventListener('mousedown', onDown)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [open])

  const pick = (fn: () => void) => () => {
    setOpen(false)
    fn()
  }

  return (
    <div className="dropdown" ref={ref}>
      <button
        className="btn btn-sm btn-outline-secondary"
        aria-expanded={open}
        aria-label="ページのメニュー"
        onClick={() => setOpen((o) => !o)}
      >
        …
      </button>
      {/* Bootstrap の JS（Popper）を使わないので、data-bs-popper="static" で CSS だけの位置決めにする（右端揃え） */}
      <ul className={`dropdown-menu dropdown-menu-end shadow${open ? ' show' : ''}`} data-bs-popper="static">
        <li>
          <button
            className="dropdown-item"
            onClick={pick(onAccess)}
            disabled={!canManageAccess}
            title={canManageAccess ? undefined : '変えられるのは、ページを作った人と管理者だけです'}
          >
            公開範囲・下書き…
            <div className="small text-body-secondary">{accessSummary}</div>
          </button>
        </li>
        <li>
          <button
            className={`dropdown-item${isLocked ? ' active' : ''}`}
            onClick={pick(() => onSettings({ is_locked: !isLocked }))}
            disabled={!canManageAccess}
            title={canManageAccess ? undefined : '変えられるのは、ページを作った人と管理者だけです'}
          >
            {isLocked ? '✓ ' : ''}🔐 ロック
            <div className="small text-body-secondary">
              {isLocked ? '作った人と管理者だけが編集できます。移動・ゴミ箱は外してから' : '作った人と管理者以外は閲覧だけにします'}
            </div>
          </button>
        </li>
        <li>
          <hr className="dropdown-divider" />
        </li>
        <li>
          <h6 className="dropdown-header">表示形式</h6>
        </li>
        {(Object.keys(LAYOUT_LABELS) as PageLayout[]).map((l) => (
          <li key={l}>
            <button
              className={`dropdown-item${layout === l ? ' active' : ''}`}
              onClick={pick(() => onSettings({ layout: l }))}
              disabled={!canEdit}
            >
              {layout === l ? '✓ ' : ''}
              {LAYOUT_LABELS[l]}
            </button>
          </li>
        ))}
        {layout === 'timeline' && (
          <>
            <li>
              <hr className="dropdown-divider" />
            </li>
            <li>
              <h6 className="dropdown-header">閲覧者から（タイムラインの投稿）</h6>
            </li>
            <li>
              <button
                className={`dropdown-item${allowGuestComments ? ' active' : ''}`}
                onClick={pick(() => onSettings({ allow_guest_comments: !allowGuestComments }))}
                disabled={!canEdit}
              >
                {allowGuestComments ? '✓ ' : ''}ゲストコメント（承認制）
                {site.guest_comments !== 'on' && <span className="small text-body-secondary">（サイト設定で停止中）</span>}
              </button>
            </li>
            <li>
              <button
                className={`dropdown-item${allowReactions ? ' active' : ''}`}
                onClick={pick(() => onSettings({ allow_reactions: !allowReactions }))}
                disabled={!canEdit}
              >
                {allowReactions ? '✓ ' : ''}リアクション
                {site.reactions !== 'on' && <span className="small text-body-secondary">（サイト設定で停止中）</span>}
              </button>
            </li>
          </>
        )}
        <li>
          <hr className="dropdown-divider" />
        </li>
        <li>
          <button
            className="dropdown-item"
            onClick={pick(() => onSettings({ is_template: !isTemplate }))}
            disabled={!canEdit}
          >
            {isTemplate ? 'テンプレートから外す' : 'テンプレートにする'}
          </button>
        </li>
        <li>
          <hr className="dropdown-divider" />
        </li>
        <li>
          <button
            className="dropdown-item text-danger"
            onClick={pick(onTrash)}
            disabled={isLocked}
            title={isLocked ? 'ロック中のページはゴミ箱へ移せません。先にロックを外してください' : undefined}
          >
            ゴミ箱へ
          </button>
        </li>
      </ul>
    </div>
  )
}
