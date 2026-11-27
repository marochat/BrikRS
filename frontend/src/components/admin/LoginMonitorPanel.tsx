import { useEffect, useMemo, useState } from 'react'
import { api, type LoginSummary } from '../../api'
import { eachDay } from '../../time'
import { DailyChart, StatTile } from './Charts'

const KIND_LABEL: Record<string, string> = {
  password: 'パスワード',
  totp: '2段階認証',
  blocked: '回数制限で拒否',
}

const fmt = (iso: string) => new Date(iso).toLocaleString('ja-JP')

/** ログイン監視：失敗の推移、IP ごとの失敗（要注意の印）、最近の失敗 */
export default function LoginMonitorPanel({ days }: { days: number }) {
  const [data, setData] = useState<LoginSummary | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    setData(null)
    setError(null)
    api.loginSummary(days).then(setData).catch((e) => setError(e.message))
  }, [days])

  const chart = useMemo(() => {
    if (!data) return null
    const byDate = new Map(data.daily_failures.map((d) => [d.date, d.failures]))
    const ds = eachDay(data.start_date, data.end_date)
    return { days: ds, series: [{ label: 'ログインの失敗', color: 'var(--series-3)', values: ds.map((d) => byDate.get(d) ?? 0) }] }
  }, [data])

  if (error) return <p className="error">{error}</p>
  if (!data || !chart) return <p className="muted">読み込み中…</p>

  return (
    <div className="vstack gap-4">
      <div className="row g-2">
        <StatTile label="試行" value={data.total_attempts} />
        <StatTile label="失敗" value={data.total_failures} />
        <StatTile label="失敗した IP" value={data.unique_failure_ips} />
        <StatTile label="試されたユーザー名" value={data.unique_attempted_usernames} />
      </div>

      <section className="card shadow-sm">
        <div className="card-header brand-font">日別のログインの失敗</div>
        <div className="card-body">
          <DailyChart title="日別のログインの失敗" days={chart.days} series={chart.series} kind="bar" />
        </div>
      </section>

      <section className="card shadow-sm">
        <div className="card-header brand-font">IP ごとの失敗</div>
        <div className="card-body p-0">
          {data.suspicious_ips.length === 0 ? (
            <p className="muted small m-3">ログインの失敗はありません</p>
          ) : (
            <div className="table-responsive">
              <table className="table table-sm table-hover align-middle mb-0 monitor-table">
                <thead>
                  <tr>
                    <th>IP アドレス</th>
                    <th className="text-end">失敗</th>
                    <th className="text-end">成功</th>
                    <th className="text-end">ユーザー名の種類</th>
                    <th>最後の試行</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {data.suspicious_ips.map((r) => (
                    <tr key={r.ip}>
                      <td className="font-monospace">{r.ip}</td>
                      <td className="text-end">{r.failures.toLocaleString()}</td>
                      <td className="text-end">{r.successes.toLocaleString()}</td>
                      <td className="text-end">{r.distinct_usernames.toLocaleString()}</td>
                      <td className="text-nowrap">{fmt(r.last_attempt)}</td>
                      <td>
                        {r.suspicious && (
                          <span className="badge badge-critical" title={`期間内に ${data.suspicious_threshold} 回以上失敗`}>
                            ⚠ 要注意
                          </span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </section>

      <section className="card shadow-sm">
        <div className="card-header brand-font">最近の失敗</div>
        <div className="card-body p-0">
          {data.recent_failures.length === 0 ? (
            <p className="muted small m-3">ログインの失敗はありません</p>
          ) : (
            <div className="table-responsive">
              <table className="table table-sm align-middle mb-0 monitor-table">
                <thead>
                  <tr>
                    <th>日時</th>
                    <th>ユーザー名</th>
                    <th>種類</th>
                    <th>IP アドレス</th>
                    <th>User-Agent</th>
                  </tr>
                </thead>
                <tbody>
                  {data.recent_failures.map((r, i) => (
                    <tr key={i}>
                      <td className="text-nowrap">{fmt(r.created_at)}</td>
                      <td>{r.username}</td>
                      <td className="text-nowrap">{KIND_LABEL[r.kind] ?? r.kind}</td>
                      <td className="font-monospace">{r.ip}</td>
                      <td className="monitor-ua" title={r.user_agent ?? ''}>
                        {r.user_agent}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </section>
      <p className="muted small m-0">
        同じ IP から期間内に {data.suspicious_threshold} 回以上失敗すると「要注意」と表示します（表示だけで、自動では止めません。
        ログインの失敗は別に、15分に10回までに制限しています）。
      </p>
    </div>
  )
}
