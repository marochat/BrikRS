import { useState } from 'react'
import { useApp } from '../AppContext'
import AccessPanel from './admin/AccessPanel'
import CommentsPanel from './admin/CommentsPanel'
import GroupsPanel from './admin/GroupsPanel'
import LoginMonitorPanel from './admin/LoginMonitorPanel'
import SitePanel from './admin/SitePanel'
import UsersAdmin from './UsersAdmin'

type Tab = 'users' | 'groups' | 'site' | 'comments' | 'access' | 'logins'

const TABS: { id: Tab; label: string }[] = [
  { id: 'users', label: 'ユーザー' },
  { id: 'groups', label: 'グループ' },
  { id: 'site', label: 'サイト' },
  { id: 'comments', label: 'コメント' },
  { id: 'access', label: 'アクセス解析' },
  { id: 'logins', label: 'ログイン監視' },
]
const PERIODS = [7, 30, 90] as const

/** 管理者の画面：ユーザー管理、グループ管理、サイトの設定、ゲストコメントの承認、アクセス解析、ログイン監視 */
export default function AdminPage() {
  const { pendingComments, refreshPendingComments } = useApp()
  const [tab, setTab] = useState<Tab>(() => (TABS.some((t) => `#${t.id}` === location.hash) ? (location.hash.slice(1) as Tab) : 'users'))
  const [days, setDays] = useState<number>(30)

  const choose = (t: Tab) => {
    setTab(t)
    history.replaceState(null, '', `#${t}`)
  }

  return (
    <div className="admin admin-wide vstack gap-3">
      <h1 className="brand-font fs-3 m-0">管理</h1>
      <div className="d-flex flex-wrap align-items-end gap-2 admin-tabs">
        <ul className="nav nav-tabs flex-grow-1">
          {TABS.map((t) => (
            <li className="nav-item" key={t.id}>
              <button
                type="button"
                className={`nav-link${tab === t.id ? ' active' : ''}`}
                aria-current={tab === t.id ? 'page' : undefined}
                onClick={() => choose(t.id)}
              >
                {t.label}
                {t.id === 'comments' && pendingComments > 0 && (
                  <span className="badge rounded-pill text-bg-danger ms-1">{pendingComments}</span>
                )}
              </button>
            </li>
          ))}
        </ul>
        {(tab === 'access' || tab === 'logins') && (
          <div className="btn-group btn-group-sm mb-1" role="group" aria-label="期間">
            {PERIODS.map((d) => (
              <button
                key={d}
                type="button"
                className={`btn btn-outline-secondary${days === d ? ' active' : ''}`}
                onClick={() => setDays(d)}
              >
                {d}日
              </button>
            ))}
          </div>
        )}
      </div>
      {tab === 'users' && <UsersAdmin />}
      {tab === 'groups' && <GroupsPanel />}
      {tab === 'site' && <SitePanel />}
      {tab === 'comments' && <CommentsPanel onChanged={refreshPendingComments} />}
      {tab === 'access' && <AccessPanel days={days} />}
      {tab === 'logins' && <LoginMonitorPanel days={days} />}
    </div>
  )
}
