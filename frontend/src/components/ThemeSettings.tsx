import { useState } from 'react'
import { api } from '../api'
import { useApp } from '../AppContext'
import { setThemePref, THEME_OPTIONS, useTheme, useThemePref, type ThemePref } from '../theme'

/** アカウント画面の「表示」欄：カラーテーマ */
export default function ThemeSettings() {
  const { me, setMe } = useApp()
  const pref = useThemePref()
  const theme = useTheme()
  const [error, setError] = useState<string | null>(null)

  const choose = async (p: ThemePref) => {
    // 画面にはすぐ反映し、アカウントへの保存は後から
    setThemePref(p)
    setError(null)
    try {
      const u = await api.setTheme(p)
      if (me) setMe({ ...me, theme: u.theme })
    } catch (e) {
      setError(`保存できませんでした（この端末では反映されています）：${(e as Error).message}`)
    }
  }

  return (
    <section className="card shadow-sm">
      <div className="card-header brand-font">表示</div>
      <div className="card-body vstack gap-2">
        <div className="form-label mb-0">カラーテーマ</div>
        <div className="btn-group flex-wrap" role="radiogroup" aria-label="カラーテーマ">
          {THEME_OPTIONS.map((o) => (
            <button
              key={o.value}
              type="button"
              role="radio"
              aria-checked={pref === o.value}
              className={`btn btn-outline-primary${pref === o.value ? ' active' : ''}`}
              onClick={() => choose(o.value)}
            >
              {o.label}
            </button>
          ))}
        </div>
        <p className="muted small">
          {pref === 'auto' ? `今は OS の設定に合わせて${theme === 'dark' ? 'ダーク' : 'ライト'}で表示しています。` : ''}
          アカウントに保存するので、別の端末でログインしても同じテーマになります。
        </p>
        {error && <p className="error">{error}</p>}
      </div>
    </section>
  )
}
