import { useState } from 'react'
import { api, getServerVersion } from '../api'
import { useApp } from '../AppContext'
import ThemeSettings from './ThemeSettings'
import TotpSettings from './TotpSettings'
import { APP_VERSION } from '../version'

export default function AccountPage() {
  const { me } = useApp()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirmPw, setConfirmPw] = useState('')
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (next !== confirmPw) {
      setMsg({ ok: false, text: '新しいパスワードが一致しません' })
      return
    }
    try {
      await api.changePassword(current, next)
      setMsg({ ok: true, text: 'パスワードを変更しました（他の端末はログアウトされます）' })
      setCurrent('')
      setNext('')
      setConfirmPw('')
    } catch (err) {
      setMsg({ ok: false, text: (err as Error).message })
    }
  }

  return (
    <div className="account vstack gap-4">
      <div>
        <h1 className="brand-font fs-3 mb-1">アカウント</h1>
        <p className="muted">
          {me?.display_name}（{me?.username}）{me?.is_admin && ' · 管理者'}
        </p>
      </div>
      <VersionNotice />
      <ThemeSettings />
      <TotpSettings />
      <form className="card shadow-sm" onSubmit={submit}>
        <div className="card-header brand-font">パスワード変更</div>
        <div className="card-body vstack gap-3">
          <label className="form-label mb-0">
            現在のパスワード
            <input
              className="form-control mt-1"
              type="password"
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
              autoComplete="current-password"
            />
          </label>
          <label className="form-label mb-0">
            新しいパスワード（10文字以上）
            <input
              className="form-control mt-1"
              type="password"
              value={next}
              onChange={(e) => setNext(e.target.value)}
              autoComplete="new-password"
            />
          </label>
          <label className="form-label mb-0">
            新しいパスワード（確認）
            <input
              className="form-control mt-1"
              type="password"
              value={confirmPw}
              onChange={(e) => setConfirmPw(e.target.value)}
              autoComplete="new-password"
            />
          </label>
          {msg && <p className={msg.ok ? 'ok' : 'error'}>{msg.text}</p>}
          <button type="submit" className="btn btn-primary align-self-start" disabled={!current || !next}>
            変更する
          </button>
        </div>
      </form>
    </div>
  )
}

/** 画面とサーバーのバージョン。食い違っていれば、更新前の古い画面を開いているので再読み込みを促す */
function VersionNotice() {
  const server = getServerVersion()
  if (server && server !== APP_VERSION)
    return (
      <div className="alert alert-warning py-2 small m-0">
        サーバーは v{server} に更新されています（この画面は v{APP_VERSION}）。
        <button className="btn btn-link btn-sm p-0 ms-1 align-baseline" onClick={() => location.reload()}>
          再読み込み
        </button>
      </div>
    )
  return <p className="muted small m-0">BrikRS v{APP_VERSION}</p>
}
