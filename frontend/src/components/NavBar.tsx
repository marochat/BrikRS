import { useState } from 'react'
import { api } from '../api'
import { useApp } from '../AppContext'
import { navigate, useRoute } from '../router'
import { flushAll } from '../saves'
import SiteLogo from './SiteLogo'

/** 上部の共通ナビゲーション。狭い画面では右のボタンで開閉し、左のボタンでページ一覧を開く */
export default function NavBar({ onOpenSidebar }: { onOpenSidebar: () => void }) {
  const { me, setMe, site, openSearch, pendingComments } = useApp()
  const route = useRoute()
  const [open, setOpen] = useState(false)

  const go = (path: string) => (e: React.MouseEvent) => {
    e.preventDefault()
    setOpen(false)
    navigate(path)
  }
  const active = (name: string) => (route.name === name ? ' active' : '')

  const logout = async () => {
    setOpen(false)
    // 保存待ちの変更を送ってからログアウトし、読み込み直す（ゲストに見えないページや、
    // ログイン中に読み込んだ下書きなどが画面に残らないように）
    await flushAll()
    try {
      await api.logout()
    } catch (e) {
      alert((e as Error).message)
      return
    }
    setMe(null)
    location.reload()
  }

  return (
    <nav className="navbar navbar-expand-lg bg-body-tertiary border-bottom sticky-top app-navbar">
      <div className="container-fluid">
        <button
          type="button"
          className="btn btn-sm d-lg-none me-1 px-2"
          onClick={onOpenSidebar}
          aria-label="ページ一覧"
        >
          ☰
        </button>
        <a className="navbar-brand me-lg-4" href="/" onClick={go('/')}>
          <SiteLogo site={site} />
        </a>
        <button
          type="button"
          className="navbar-toggler btn-sm"
          aria-expanded={open}
          aria-controls="navbar-menu"
          aria-label="メニュー"
          onClick={() => setOpen((o) => !o)}
        >
          <span className="navbar-toggler-icon" />
        </button>
        <div className={`collapse navbar-collapse${open ? ' show' : ''}`} id="navbar-menu">
          <ul className="navbar-nav me-auto">
            <li className="nav-item">
              <a className={`nav-link brand-font${active('recent')}`} href="/recent" onClick={go('/recent')}>
                最近の更新
              </a>
            </li>
            <li className="nav-item">
              <button
                type="button"
                className="nav-link brand-font"
                onClick={() => {
                  setOpen(false)
                  openSearch()
                }}
              >
                検索 <kbd className="ms-1 d-none d-lg-inline">Ctrl+K</kbd>
              </button>
            </li>
          </ul>
          <ul className="navbar-nav align-items-lg-center gap-lg-1">
            {me ? (
              <>
                <li className="nav-item">
                  <a className={`nav-link brand-font${active('account')}`} href="/account" onClick={go('/account')}>
                    アカウント <span className="text-body-secondary small ms-1">{me.display_name}</span>
                  </a>
                </li>
                <li className="nav-item">
                  <a className={`nav-link brand-font${active('trash')}`} href="/trash" onClick={go('/trash')}>
                    ゴミ箱
                  </a>
                </li>
                {me.is_admin && (
                  <li className="nav-item">
                    <a
                      className={`nav-link brand-font${active('users')}`}
                      href={pendingComments ? '/admin/users#comments' : '/admin/users'}
                      onClick={go(pendingComments ? '/admin/users#comments' : '/admin/users')}
                    >
                      管理
                      {pendingComments > 0 && (
                        <span className="badge rounded-pill text-bg-danger ms-1" title="承認待ちのコメント">
                          {pendingComments}
                          <span className="visually-hidden">件の承認待ちコメント</span>
                        </span>
                      )}
                    </a>
                  </li>
                )}
                <li className="nav-item ms-lg-2 my-2 my-lg-0">
                  <button type="button" className="btn btn-sm btn-outline-secondary brand-font" onClick={logout}>
                    ログアウト
                  </button>
                </li>
              </>
            ) : (
              <li className="nav-item my-2 my-lg-0">
                <a className="btn btn-sm btn-outline-primary brand-font" href="/login" onClick={go('/login')}>
                  ログイン
                </a>
              </li>
            )}
          </ul>
        </div>
      </div>
    </nav>
  )
}
