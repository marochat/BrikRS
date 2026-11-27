import type { SiteSettings } from '../api'
import { APP_VERSION } from '../version'

/** ロゴの下に出す一文（無ければ空）。バージョンはマイナー番号まで（v0.4） */
function taglineText(site: SiteSettings): string {
  if (site.tagline === 'powered') return `BrikRS v${APP_VERSION.split('.').slice(0, 2).join('.')}`
  if (site.tagline === 'custom') return site.tagline_text
  return ''
}

/** ナビバーのタイトルロゴ：アイコンとサイトのタイトル、その下に小さな一文 */
export default function SiteLogo({ site }: { site: SiteSettings }) {
  const tagline = taglineText(site)
  return (
    <span className="site-logo">
      <img src="/icon-192.png" alt="" width={32} height={32} />
      <span className="site-logo-text">
        <span className="site-logo-title">{site.title}</span>
        {tagline && <span className="site-logo-tagline">{tagline}</span>}
      </span>
    </span>
  )
}
