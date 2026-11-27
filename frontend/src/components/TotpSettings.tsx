import { useEffect, useState } from 'react'
import { api, type TotpSetup } from '../api'
import { useApp } from '../AppContext'

type Mode = 'idle' | 'setup' | 'disable' | 'regenerate'

/** アカウント画面の「2段階認証」欄 */
export default function TotpSettings() {
  const { me, setMe } = useApp()
  const [status, setStatus] = useState<{ enabled: boolean; recovery_codes_left: number } | null>(null)
  const [mode, setMode] = useState<Mode>('idle')
  const [setup, setSetup] = useState<TotpSetup | null>(null)
  const [code, setCode] = useState('')
  const [password, setPassword] = useState('')
  // 発行直後のリカバリーコード。この画面を閉じると二度と表示できない
  const [recovery, setRecovery] = useState<string[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = () =>
    api
      .totpStatus()
      .then(setStatus)
      .catch((e) => setError(e.message))
  useEffect(() => {
    load()
  }, [])

  const reset = (m: Mode = 'idle') => {
    setMode(m)
    setCode('')
    setPassword('')
    setError(null)
  }

  const run = async (fn: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await fn()
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const startSetup = () =>
    run(async () => {
      setSetup(await api.totpSetup())
      reset('setup')
    })

  const enable = (e: React.FormEvent) => {
    e.preventDefault()
    run(async () => {
      const r = await api.totpEnable(setup!.secret, code)
      setRecovery(r.recovery_codes)
      setSetup(null)
      reset()
      if (me) setMe({ ...me, totp_enabled: true })
      await load()
    })
  }

  const disable = (e: React.FormEvent) => {
    e.preventDefault()
    run(async () => {
      await api.totpDisable(password, code)
      reset()
      if (me) setMe({ ...me, totp_enabled: false })
      await load()
    })
  }

  const regenerate = (e: React.FormEvent) => {
    e.preventDefault()
    run(async () => {
      setRecovery((await api.totpRegenerate(code)).recovery_codes)
      reset()
      await load()
    })
  }

  if (!status) return error ? <p className="error">{error}</p> : null

  if (recovery) return <RecoveryCodes codes={recovery} onDone={() => setRecovery(null)} />

  const codeInput = (
    <label className="form-label mb-0">
      認証アプリの6桁のコード{mode !== 'setup' && '（またはリカバリーコード）'}
      <input
        className="form-control mt-1"
        value={code}
        onChange={(e) => setCode(e.target.value)}
        inputMode="numeric"
        autoComplete="one-time-code"
        maxLength={20}
        autoFocus={mode !== 'disable'}
      />
    </label>
  )

  return (
    <section className="card shadow-sm">
      <div className="card-header brand-font">2段階認証</div>
      <div className="card-body vstack gap-3">
        {!status.enabled && mode === 'idle' && (
          <>
            <p className="muted small">
              ログインのときに、パスワードに加えて認証アプリ（Google Authenticator、Microsoft Authenticator、1Password
              など）のコードを求めます。パスワードが漏れても、スマートフォンが無ければログインできなくなります。
            </p>
            <p>
              状態：<strong>無効</strong>
            </p>
            <button className="btn btn-primary align-self-start" onClick={startSetup} disabled={busy}>
              設定する
            </button>
          </>
        )}

        {mode === 'setup' && setup && (
          <form className="totp-setup" onSubmit={enable}>
            <p className="small">1. 認証アプリでこの QR コードを読み取ってください。</p>
            <img
              className="totp-qr"
              src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(setup.qr_svg)}`}
              width={200}
              height={200}
              alt="認証アプリ登録用の QR コード"
            />
            <details className="small">
              <summary>QR コードを読み取れないとき</summary>
              <p className="muted">アプリで「キーを入力」を選び、次のキーを入力してください（時間ベース）。</p>
              <code className="totp-secret">{setup.secret.replace(/(.{4})/g, '$1 ').trim()}</code>
            </details>
            <p className="small">2. アプリに表示された6桁のコードを入力してください。</p>
            {codeInput}
            {error && <p className="error">{error}</p>}
            <div className="button-row">
              <button type="submit" className="btn btn-primary" disabled={busy || !code.trim()}>
                有効にする
              </button>
              <button type="button" className="btn btn-outline-secondary" onClick={() => (setSetup(null), reset())}>
                やめる
              </button>
            </div>
          </form>
        )}

        {status.enabled && mode === 'idle' && (
          <>
            <p>
              状態：<strong className="ok">有効</strong>
            </p>
            <p className={`small ${status.recovery_codes_left <= 3 ? 'error' : 'muted'}`}>
              未使用のリカバリーコード：{status.recovery_codes_left} 個
              {status.recovery_codes_left <= 3 && '（少なくなっています。作り直してください）'}
            </p>
            <div className="button-row">
              <button className="btn btn-outline-secondary" onClick={() => reset('regenerate')}>
                リカバリーコードを作り直す
              </button>
              <button className="btn btn-outline-danger" onClick={() => reset('disable')}>
                解除する
              </button>
            </div>
          </>
        )}

        {mode === 'regenerate' && (
          <form className="totp-setup" onSubmit={regenerate}>
            <p className="muted small">作り直すと、今までのリカバリーコードは使えなくなります。</p>
            {codeInput}
            {error && <p className="error">{error}</p>}
            <div className="button-row">
              <button type="submit" className="btn btn-primary" disabled={busy || !code.trim()}>
                作り直す
              </button>
              <button type="button" className="btn btn-outline-secondary" onClick={() => reset()}>
                やめる
              </button>
            </div>
          </form>
        )}

        {mode === 'disable' && (
          <form className="totp-setup" onSubmit={disable}>
            <p className="muted small">解除すると、パスワードだけでログインできるようになります。</p>
            <label className="form-label mb-0">
              パスワード
              <input
                className="form-control mt-1"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="current-password"
                autoFocus
              />
            </label>
            {codeInput}
            {error && <p className="error">{error}</p>}
            <div className="button-row">
              <button type="submit" className="btn btn-danger" disabled={busy || !password || !code.trim()}>
                解除する
              </button>
              <button type="button" className="btn btn-outline-secondary" onClick={() => reset()}>
                やめる
              </button>
            </div>
          </form>
        )}
      </div>
    </section>
  )
}

function RecoveryCodes({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  const [copied, setCopied] = useState(false)
  const text = `BrikRS リカバリーコード（${location.host}）\n各コードは1回だけ使えます。\n\n${codes.join('\n')}\n`

  const copy = async () => {
    await navigator.clipboard.writeText(text)
    setCopied(true)
  }

  const download = () => {
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([text], { type: 'text/plain' }))
    a.download = 'brikrs-recovery-codes.txt'
    a.click()
    URL.revokeObjectURL(a.href)
  }

  return (
    <section className="card shadow-sm border-warning">
      <div className="card-header brand-font">リカバリーコード</div>
      <div className="card-body vstack gap-3">
        <p className="small">
          認証アプリが使えなくなったとき（機種変更・紛失など）に、コードの代わりに入力します。
          <strong>この画面を閉じると二度と表示できません。</strong>
          パスワードマネージャーなど、安全な場所に保存してください。各コードは1回だけ使えます。
        </p>
        <ul className="recovery-codes">
          {codes.map((c) => (
            <li key={c}>{c}</li>
          ))}
        </ul>
        <div className="button-row">
          <button className="btn btn-outline-secondary" onClick={copy}>
            {copied ? 'コピーしました' : 'コピー'}
          </button>
          <button className="btn btn-outline-secondary" onClick={download}>
            テキストで保存
          </button>
        </div>
        <button className="btn btn-primary" onClick={onDone}>
          保存しました
        </button>
      </div>
    </section>
  )
}
