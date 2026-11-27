import { useEffect, useMemo, useState } from 'react'
import { api, type AccessSummary } from '../../api'
import { displayTitle } from '../../AppContext'
import { navigate } from '../../router'
import { eachDay } from '../../time'
import { DailyChart, MeterList, StatTile } from './Charts'

/** このサイトの中のパスか（`//host`・`/\host` は別のサイトになる。0.16.0 までに記録された値もあるので表示でも確かめる） */
const isSitePath = (path: string) => path.startsWith('/') && !path.startsWith('//') && !path.includes('\\')

/** アクセス解析：閲覧者（bot と編集者を除く）のページビュー・訪問者・よく見られているページ・参照元 */
export default function AccessPanel({ days }: { days: number }) {
  const [data, setData] = useState<AccessSummary | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setData(null)
    setError(null)
    api.accessSummary(days).then(setData).catch((e) => setError(e.message))
  }, [days])

  const chart = useMemo(() => {
    if (!data) return null
    const byDate = new Map(data.daily.map((d) => [d.date, d]))
    const ds = eachDay(data.start_date, data.end_date)
    return {
      days: ds,
      series: [
        { label: 'ページビュー', color: 'var(--series-1)', values: ds.map((d) => byDate.get(d)?.pageviews ?? 0) },
        { label: 'ユニーク訪問者', color: 'var(--series-2)', values: ds.map((d) => byDate.get(d)?.unique_visitors ?? 0) },
      ],
    }
  }, [data])

  if (error) return <p className="error">{error}</p>
  if (!data || !chart) return <p className="muted">読み込み中…</p>

  return (
    <div className="vstack gap-4">
      <div className="row g-2">
        <StatTile label="ページビュー" value={data.total_pageviews} />
        <StatTile label="ユニーク訪問者" value={data.total_unique_visitors} />
        <StatTile label="セッション" value={data.total_sessions} />
        <StatTile label="bot（除外した数）" value={data.bot_pageviews} />
      </div>

      <section className="card shadow-sm">
        <div className="card-header brand-font">日別の推移</div>
        <div className="card-body">
          <DailyChart title="日別の推移" days={chart.days} series={chart.series} kind="line" />
        </div>
      </section>

      <div className="row g-3">
        <section className="col-lg-7">
          <div className="card shadow-sm h-100">
            <div className="card-header brand-font">よく見られているページ</div>
            <div className="card-body">
              <MeterList
                color="var(--series-1)"
                empty="まだ記録がありません"
                items={data.top_pages.map((p) => ({
                  key: p.path,
                  title: `${p.path}（ユニーク訪問者 ${p.unique_visitors.toLocaleString()}）`,
                  value: p.pageviews,
                  label: isSitePath(p.path) ? (
                    <a
                      href={p.path}
                      onClick={(e) => {
                        e.preventDefault()
                        navigate(p.path)
                      }}
                    >
                      {p.title !== null ? `${p.icon || '📄'} ${displayTitle(p.title)}` : p.path}
                    </a>
                  ) : (
                    <span>{p.path}</span>
                  ),
                }))}
              />
            </div>
          </div>
        </section>
        <section className="col-lg-5">
          <div className="card shadow-sm h-100">
            <div className="card-header brand-font">参照元</div>
            <div className="card-body">
              <MeterList
                color="var(--series-1)"
                empty="外部のサイトからの訪問はまだありません"
                items={data.referrers.map((r) => ({ key: r.host, label: r.host, value: r.pageviews }))}
              />
            </div>
          </div>
        </section>
      </div>
      <p className="muted small m-0">
        ログインしている編集者の閲覧と、bot・クローラー（User-Agent で判定）は数えていません。記録は一定期間（既定
        180日）で自動的に削除されます。
      </p>
    </div>
  )
}
