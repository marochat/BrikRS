import { useState } from 'react'
import { api } from '../api'
import { useApp } from '../AppContext'

/**
 * ログインしたら読み込み直す（ログアウトと同じ）。ページの一覧などはゲストとして読んだものなので、
 * そのままでは下書き・公開範囲を限ったページが出ない
 */
const reloadAfterLogin = () => location.replace('/')

export default function LoginPage() {
  const { me } = useApp()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  // 2段階認証が有効なユーザーは、パスワードの後にコードを入れる
  const [mfaToken, setMfaToken] = useState<string | null>(null)
  const [code, setCode] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  if (me) return <div className="message">{me.display_name} としてログイン中です</div>

  const run = async (fn: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await fn()
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const submitPassword = (e: React.FormEvent) => {
    e.preventDefault()
    run(async () => {
      const r = await api.login(username, password)
      if ('mfaToken' in r) {
        setMfaToken(r.mfaToken)
        setPassword('')
        return
      }
      if (r.user) reloadAfterLogin()
    })
  }

  const submitCode = (e: React.FormEvent) => {
    e.preventDefault()
    run(async () => {
      try {
        if (await api.loginTotp(mfaToken!, code)) reloadAfterLogin()
      } catch (err) {
        setCode('')
        // 期限切れ・試行回数の上限なら、パスワードからやり直す
        if ((err as Error).message.includes('もう一度ログイン')) setMfaToken(null)
        throw err
      }
    })
  }

  if (mfaToken)
    return (
      <form className="card shadow-sm form-panel" onSubmit={submitCode}>
        <div className="card-header text-center brand-font fs-5">2段階認証</div>
        <div className="card-body vstack gap-3">
          <p className="muted">認証アプリに表示されている6桁のコードを入力してください。</p>
          <label className="form-label mb-0">
            確認コード
            <input
              className="form-control mt-1"
              value={code}
              onChange={(e) => setCode(e.target.value)}
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={20}
              autoFocus
            />
          </label>
          {error && <p className="error">{error}</p>}
          <button type="submit" className="btn btn-primary" disabled={busy || !code.trim()}>
            確認
          </button>
          <p className="muted small">認証アプリが使えないときは、リカバリーコード（xxxx-xxxx）を入力してください。</p>
          <button
            type="button"
            className="btn btn-link btn-sm"
            onClick={() => (setMfaToken(null), setCode(''), setError(null))}
          >
            ← ログインに戻る
          </button>
        </div>
      </form>
    )

  return (
    <form className="card shadow-sm form-panel" onSubmit={submitPassword}>
      <div className="card-header text-center brand-font fs-5">ログイン</div>
      <div className="card-body vstack gap-3">
        <label className="form-label mb-0">
          ユーザー名
          <input
            className="form-control mt-1"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoComplete="username"
            autoFocus
          />
        </label>
        <label className="form-label mb-0">
          パスワード
          <input
            className="form-control mt-1"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
          />
        </label>
        {error && <p className="error">{error}</p>}
        <button type="submit" className="btn btn-primary" disabled={busy || !username || !password}>
          ログイン
        </button>
      </div>
    </form>
  )
}
