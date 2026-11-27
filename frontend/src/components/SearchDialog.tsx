import { useEffect, useState, type ReactNode } from 'react'
import { api, type SearchHit } from '../api'
import { displayTitle } from '../AppContext'
import { navigate, pagePath } from '../router'
import Modal from './Modal'

const DEBOUNCE_MS = 200

/** 検索語を <mark> で囲む（innerHTML は使わない） */
function highlight(text: string, terms: string[]): ReactNode {
  const ts = terms.filter(Boolean).map((t) => t.toLowerCase())
  if (!ts.length) return text
  const out: ReactNode[] = []
  const lower = text.toLowerCase()
  let i = 0
  while (i < text.length) {
    let best: { pos: number; len: number } | null = null
    for (const t of ts) {
      const pos = lower.indexOf(t, i)
      if (pos >= 0 && (!best || pos < best.pos)) best = { pos, len: t.length }
    }
    if (!best) break
    if (best.pos > i) out.push(text.slice(i, best.pos))
    out.push(<mark key={best.pos}>{text.slice(best.pos, best.pos + best.len)}</mark>)
    i = best.pos + best.len
  }
  out.push(text.slice(i))
  return out
}

export default function SearchDialog({ initialQuery = '', onClose }: { initialQuery?: string; onClose: () => void }) {
  const [q, setQ] = useState(initialQuery)
  const [hits, setHits] = useState<SearchHit[]>([])
  const [searched, setSearched] = useState('')
  const [selected, setSelected] = useState(0)

  useEffect(() => {
    const query = q.trim()
    const t = window.setTimeout(async () => {
      try {
        setHits(query ? await api.search(query) : [])
        setSearched(query)
        setSelected(0)
      } catch (e) {
        console.error(e)
      }
    }, DEBOUNCE_MS)
    return () => window.clearTimeout(t)
  }, [q])

  const open = (h: SearchHit) => {
    navigate(pagePath(h.id))
    onClose()
  }

  const terms = searched.split(/\s+/)

  return (
    <Modal onClose={onClose} className="search-modal" dialogClassName="modal-lg modal-fullscreen-sm-down search-dialog">
      <input
        className="form-control form-control-lg border-0 border-bottom rounded-0 shadow-none search-input"
        autoFocus
        placeholder="ページを検索（スペース区切りで AND 検索）"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown') {
            e.preventDefault()
            setSelected((s) => Math.min(s + 1, hits.length - 1))
          } else if (e.key === 'ArrowUp') {
            e.preventDefault()
            setSelected((s) => Math.max(s - 1, 0))
          } else if (e.key === 'Enter' && hits[selected]) {
            open(hits[selected])
          }
        }}
      />
      <ul className="search-results">
        {hits.map((h, i) => (
          <li key={h.id}>
            <button className={i === selected ? 'selected' : ''} onMouseEnter={() => setSelected(i)} onClick={() => open(h)}>
              <span className="search-title">
                <span className="tree-icon">{h.icon || '📄'}</span>
                {highlight(displayTitle(h.title), terms)}
              </span>
              {h.snippet && <span className="search-snippet">{highlight(h.snippet, terms)}</span>}
            </button>
          </li>
        ))}
        {searched && hits.length === 0 && <li className="search-empty">見つかりませんでした</li>}
      </ul>
    </Modal>
  )
}
