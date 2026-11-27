import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'

/*
 * 管理画面の小さなグラフ（外部ライブラリは使わず SVG で描く）。
 * 色は .viz の CSS 変数（--series-1 など）で、ライト / ダークそれぞれ検証済みの段を使う（index.css）。
 * 数値や文字は系列の色ではなく文字の色で書き、系列は凡例と線の端のラベルで見分けられるようにする。
 */

export function StatTile({ label, value, note }: { label: string; value: number; note?: string }) {
  return (
    <div className="col-6 col-md-3">
      <div className="stat-tile">
        <div className="stat-label">{label}</div>
        <div className="stat-value">{value.toLocaleString()}</div>
        {note && <div className="stat-note">{note}</div>}
      </div>
    </div>
  )
}

/** 目盛りの上限を切りのよい値に（1・2・5 の桁） */
function niceMax(v: number): number {
  if (v <= 4) return 4
  const m = 10 ** Math.floor(Math.log10(v))
  const r = v / m
  return (r <= 1 ? 1 : r <= 2 ? 2 : r <= 5 ? 5 : 10) * m
}

export type Series = { label: string; color: string; values: number[] }

const H = 220
const M = { top: 12, right: 44, bottom: 24, left: 40 }
const PH = H - M.top - M.bottom

/** 表示されている幅（px）。グラフをその幅で描いて、文字が縮まないようにする */
function useWidth(fallback: number) {
  const ref = useRef<HTMLElement>(null)
  const [w, setW] = useState(fallback)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const ro = new ResizeObserver(([e]) => setW(Math.max(260, Math.round(e.contentRect.width))))
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  return [ref, w] as const
}

/**
 * 日別の推移。line：複数の系列の折れ線（凡例と線の端のラベル付き）、bar：1つの系列の縦棒。
 * なぞると縦線と値の吹き出しを出す。「表で見る」で同じ数字を表で見られる
 */
export function DailyChart({
  days,
  series,
  kind,
  title,
}: {
  days: string[]
  series: Series[]
  kind: 'line' | 'bar'
  title: string
}) {
  const [hover, setHover] = useState<number | null>(null)
  const [asTable, setAsTable] = useState(false)
  const [box, W] = useWidth(640)
  const PW = W - M.left - M.right
  const max = useMemo(() => niceMax(Math.max(1, ...series.flatMap((s) => s.values))), [series])
  const n = days.length
  const slot = PW / Math.max(1, n)
  const x = (i: number) => (kind === 'bar' ? slot * i + slot / 2 : n <= 1 ? PW / 2 : (i / (n - 1)) * PW)
  const y = (v: number) => PH - (v / max) * PH
  const ticks = [0, max / 2, max]
  const xLabels = [...new Set([0, Math.floor((n - 1) / 2), n - 1])].filter((i) => i >= 0)
  const total = series.map((s) => s.values.reduce((a, b) => a + b, 0))

  if (n === 0) return null

  return (
    <figure className="viz m-0" ref={box}>
      <div className="d-flex align-items-center gap-3 small mb-1 flex-wrap">
        {series.length > 1 &&
          series.map((s) => (
            <span key={s.label} className="viz-legend">
              <span className="viz-swatch" style={{ background: s.color }} />
              {s.label}
            </span>
          ))}
        <button type="button" className="btn btn-link btn-sm p-0 ms-auto" onClick={() => setAsTable((t) => !t)}>
          {asTable ? 'グラフで見る' : '表で見る'}
        </button>
      </div>

      {asTable ? (
        <div className="table-responsive viz-table">
          <table className="table table-sm mb-0">
            <thead>
              <tr>
                <th>日付</th>
                {series.map((s) => (
                  <th key={s.label} className="text-end">
                    {s.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {days.map((d, i) => (
                <tr key={d}>
                  <td>{d}</td>
                  {series.map((s) => (
                    <td key={s.label} className="text-end">
                      {s.values[i].toLocaleString()}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="position-relative">
          <svg
            viewBox={`0 0 ${W} ${H}`}
            className="viz-svg"
            role="img"
            aria-label={`${title}：${series.map((s, i) => `${s.label} 合計 ${total[i]}`).join('、')}`}
          >
            <g transform={`translate(${M.left},${M.top})`}>
              {ticks.map((t) => (
                <g key={t}>
                  <line x1={0} x2={PW} y1={y(t)} y2={y(t)} className={t === 0 ? 'viz-axis' : 'viz-grid'} />
                  <text x={-8} y={y(t)} textAnchor="end" dominantBaseline="middle" className="viz-tick">
                    {Math.round(t).toLocaleString()}
                  </text>
                </g>
              ))}
              {xLabels.map((i) => (
                <text
                  key={i}
                  x={x(i)}
                  y={PH + 16}
                  textAnchor={kind === 'bar' ? 'middle' : i === 0 ? 'start' : i === n - 1 ? 'end' : 'middle'}
                  className="viz-tick"
                >
                  {days[i].slice(5)}
                </text>
              ))}

              {kind === 'bar'
                ? series[0].values.map((v, i) => {
                    // 4px の角丸は上端だけ。棒どうしは 2px 空ける
                    const w = Math.max(1, slot - 2)
                    const h = (v / max) * PH
                    if (v === 0) return null
                    const r = Math.min(4, w / 2, h)
                    const x0 = x(i) - w / 2
                    const y0 = PH - h
                    return (
                      <path
                        key={i}
                        d={`M${x0},${PH} V${y0 + r} Q${x0},${y0} ${x0 + r},${y0} H${x0 + w - r} Q${x0 + w},${y0} ${x0 + w},${y0 + r} V${PH} Z`}
                        fill={series[0].color}
                        opacity={hover === null || hover === i ? 1 : 0.55}
                      />
                    )
                  })
                : series.map((s) => (
                    <g key={s.label}>
                      <path
                        d={s.values.map((v, i) => `${i ? 'L' : 'M'}${x(i)},${y(v)}`).join(' ')}
                        fill="none"
                        stroke={s.color}
                        strokeWidth={2}
                        strokeLinejoin="round"
                        strokeLinecap="round"
                      />
                      {/* 線の端：面の色の輪をつけた丸印と、最後の値（色だけに頼らず読めるように） */}
                      <circle cx={x(n - 1)} cy={y(s.values[n - 1])} r={6} className="viz-ring" />
                      <circle cx={x(n - 1)} cy={y(s.values[n - 1])} r={4} fill={s.color} />
                      <text x={x(n - 1) + 9} y={y(s.values[n - 1])} dominantBaseline="middle" className="viz-end">
                        {s.values[n - 1].toLocaleString()}
                      </text>
                    </g>
                  ))}

              {hover !== null && kind === 'line' && (
                <>
                  <line x1={x(hover)} x2={x(hover)} y1={0} y2={PH} className="viz-crosshair" />
                  {series.map((s) => (
                    <g key={s.label}>
                      <circle cx={x(hover)} cy={y(s.values[hover])} r={6} className="viz-ring" />
                      <circle cx={x(hover)} cy={y(s.values[hover])} r={4} fill={s.color} />
                    </g>
                  ))}
                </>
              )}
              {/* なぞる範囲は印より広く、描画域全体にする */}
              <rect
                x={0}
                y={0}
                width={PW}
                height={PH}
                fill="transparent"
                onMouseMove={(e) => {
                  const r = e.currentTarget.getBoundingClientRect()
                  const ratio = (e.clientX - r.left) / r.width
                  const i = kind === 'bar' ? Math.floor(ratio * n) : Math.round(ratio * (n - 1))
                  setHover(Math.max(0, Math.min(n - 1, i)))
                }}
                onMouseLeave={() => setHover(null)}
              />
            </g>
          </svg>
          {hover !== null && (
            <div
              className="viz-tooltip"
              style={{ left: `${((M.left + x(hover)) / W) * 100}%`, transform: `translateX(${hover > n / 2 ? '-105%' : '5%'})` }}
            >
              <div className="viz-tooltip-date">{days[hover]}</div>
              {series.map((s) => (
                <div key={s.label}>
                  <span className="viz-swatch" style={{ background: s.color }} />
                  {s.label}：<strong>{s.values[hover].toLocaleString()}</strong>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </figure>
  )
}

/** 量の多い順の横棒（1つの系列。値は棒の右に数字で書く） */
export function MeterList({
  items,
  color,
  empty,
}: {
  items: { key: string; label: ReactNode; title?: string; value: number }[]
  color: string
  empty: string
}) {
  const max = Math.max(1, ...items.map((i) => i.value))
  if (items.length === 0) return <p className="muted small">{empty}</p>
  return (
    <ul className="viz meter-list">
      {items.map((it) => (
        <li key={it.key} title={it.title ?? it.value.toLocaleString()}>
          <span className="meter-label">{it.label}</span>
          <span className="meter-track">
            <span className="meter-bar" style={{ width: `${(it.value / max) * 100}%`, background: color }} />
          </span>
          <span className="meter-value">{it.value.toLocaleString()}</span>
        </li>
      ))}
    </ul>
  )
}
