import { useState, type FormEvent } from 'react'
import { api, type SiteSettings, type SiteTagline, type Switch } from '../../api'
import { useApp } from '../../AppContext'
import SiteLogo from '../SiteLogo'

/** アップロード前の画像の縮小：長い辺の上限（px）。0 は縮小しない */
const IMAGE_SIZES = [0, 1280, 1600, 1920, 2560, 3840]

const TAGLINES: { value: SiteTagline; label: string }[] = [
  { value: 'none', label: '無し' },
  { value: 'powered', label: 'BrikRS のバージョン' },
  { value: 'custom', label: '自由に書く' },
]

/** 管理画面の「サイト」タブ：タイトルロゴとその下の一文、閲覧者からのコメント・リアクション */
export default function SitePanel() {
  const { site, setSite } = useApp()
  const [form, setForm] = useState<SiteSettings>(site)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [saved, setSaved] = useState(false)

  const change = (patch: Partial<SiteSettings>) => {
    setForm((f) => ({ ...f, ...patch }))
    setSaved(false)
  }

  const save = async (e: FormEvent) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const s = await api.updateSite(form)
      setSite(s)
      setForm(s)
      setSaved(true)
    } catch (err) {
      setError((err as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const unchanged = JSON.stringify(form) === JSON.stringify(site)

  const saveRow = (
    <div className="d-flex align-items-center gap-2">
      <button type="submit" className="btn btn-primary" disabled={busy || unchanged || !form.title.trim()}>
        保存
      </button>
      {saved && <span className="muted small">保存しました</span>}
      {error && <span className="error">{error}</span>}
    </div>
  )

  return (
    <form className="vstack gap-3" onSubmit={save}>
      <div className="card shadow-sm">
        <div className="card-header brand-font">タイトルロゴ</div>
        <div className="card-body vstack gap-3">
          <label className="form-label mb-0">
            サイトのタイトル（60文字まで）
            <input
              className="form-control mt-1"
              value={form.title}
              maxLength={60}
              onChange={(e) => change({ title: e.target.value })}
            />
            <span className="form-text">ナビバーのロゴのほか、ブラウザのタブ、フィード、共有カード（OGP）のサイト名になります。</span>
          </label>
          <div>
            <div className="form-label">ロゴの下の一文</div>
            <div className="btn-group flex-wrap" role="radiogroup" aria-label="ロゴの下の一文">
              {TAGLINES.map((t) => (
                <button
                  key={t.value}
                  type="button"
                  role="radio"
                  aria-checked={form.tagline === t.value}
                  className={`btn btn-outline-primary${form.tagline === t.value ? ' active' : ''}`}
                  onClick={() => change({ tagline: t.value })}
                >
                  {t.label}
                </button>
              ))}
            </div>
            {form.tagline === 'custom' && (
              <input
                className="form-control mt-2"
                value={form.tagline_text}
                maxLength={100}
                placeholder="例：日々の記録と覚え書き"
                aria-label="ロゴの下に出す一文"
                onChange={(e) => change({ tagline_text: e.target.value })}
              />
            )}
          </div>
          <div>
            <div className="form-label">プレビュー</div>
            <div className="site-logo-preview">
              <SiteLogo site={{ ...form, title: form.title.trim() || 'BrikRS' }} />
            </div>
          </div>
          {saveRow}
        </div>
      </div>
      <div className="card shadow-sm">
        <div className="card-header brand-font">閲覧者からのコメント・リアクション</div>
        <div className="card-body vstack gap-3">
          <p className="form-text m-0">
            ここで有効にしたうえで、タイムラインのページごとに「…」メニューから受け付けを選びます。止めると、すべてのページで表示も受け付けもしなくなります（データは残ります）。
          </p>
          <SwitchRow
            label="ゲストコメント（承認制）"
            help="ログインしていない人が、タイムラインの投稿にコメントできます。管理者が「管理 › コメント」で承認するまで公開されません。"
            value={form.guest_comments}
            onChange={(v) => change({ guest_comments: v })}
          />
          <SwitchRow
            label="コメントの画像認証（CAPTCHA）"
            help="コメントの送信時に、画像の文字を入力してもらいます（このサーバーで画像を作ります。外部のサービスは使いません）。"
            value={form.comment_captcha}
            disabled={form.guest_comments !== 'on'}
            onChange={(v) => change({ comment_captcha: v })}
          />
          <SwitchRow
            label="リアクション"
            help="誰でも、タイムラインの投稿・返信に 👍 ❤️ 😂 😮 🎉 を付けられます。件数は誰にでも表示されます。"
            value={form.reactions}
            onChange={(v) => change({ reactions: v })}
          />
          {saveRow}
        </div>
      </div>
      <div className="card shadow-sm">
        <div className="card-header brand-font">画像のアップロード</div>
        <div className="card-body vstack gap-3">
          <label className="form-label mb-0">
            大きな画像の縮小（長い辺の上限）
            <select
              className="form-select mt-1"
              style={{ maxWidth: '16rem' }}
              value={form.image_max_px}
              onChange={(e) => change({ image_max_px: e.target.value })}
            >
              {[...new Set([...IMAGE_SIZES, Number(form.image_max_px) || 0])]
                .sort((a, b) => a - b)
                .map((n) => (
                  <option key={n} value={String(n)}>
                    {n ? `${n} px` : '縮小しない'}
                  </option>
                ))}
            </select>
            <span className="form-text">
              長い辺がこれより大きい写真（JPEG・PNG・WebP）は、ブラウザで縮小してから送ります。転送量と保存する容量が減り、撮影位置などの Exif も残りません。GIF（動く画像）はそのまま送ります。ファイルの大きさの上限（10MB）は縮小した後の大きさで判定します。
            </span>
          </label>
          {saveRow}
        </div>
      </div>
    </form>
  )
}

function SwitchRow({
  label,
  help,
  value,
  disabled = false,
  onChange,
}: {
  label: string
  help: string
  value: Switch
  disabled?: boolean
  onChange: (v: Switch) => void
}) {
  return (
    <div className="form-check form-switch m-0">
      <label className="form-check-label">
        <input
          className="form-check-input"
          type="checkbox"
          role="switch"
          checked={value === 'on'}
          disabled={disabled}
          onChange={(e) => onChange(e.target.checked ? 'on' : 'off')}
        />
        {label}
      </label>
      <div className="form-text mt-0">{help}</div>
    </div>
  )
}
