import { useCallback, useEffect, useMemo, useState } from 'react'
import { api, type DbColumn, type RowSummary } from '../api'
import { displayTitle } from '../AppContext'
import { navigate, pagePath } from '../router'
import { safeUrl } from './BlockView'
import DatabaseColumnsDialog from './DatabaseColumnsDialog'

type SortKey = { col: string; dir: 1 | -1 } | null

type Props = {
  pageId: number
  initialSchema: DbColumn[]
  /** 行の追加・行の編集（ロック中の行は、作った人と管理者だけ） */
  editable: boolean
  /** 列の編集（データベースページ自身がロック中なら、作った人と管理者だけ） */
  schemaEditable: boolean
}

/** データベースページ：子ページを「行」として、型付きの列を持つ表で並べる */
export default function DatabaseTable({ pageId, initialSchema, editable, schemaEditable }: Props) {
  const [schema, setSchema] = useState<DbColumn[]>(initialSchema)
  const [rows, setRows] = useState<RowSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [sort, setSort] = useState<SortKey>(null)
  const [columnsOpen, setColumnsOpen] = useState(false)
  const [adding, setAdding] = useState(false)

  const load = useCallback(
    () =>
      api
        .rows(pageId)
        .then(setRows)
        .catch((e) => setError((e as Error).message)),
    [pageId],
  )
  useEffect(() => {
    setLoading(true)
    load().finally(() => setLoading(false))
  }, [load])

  const addRow = async () => {
    setAdding(true)
    try {
      await api.createPage(pageId)
      await load()
    } catch (e) {
      alert((e as Error).message)
    } finally {
      setAdding(false)
    }
  }

  const saveColumns = async (next: DbColumn[]) => {
    const p = await api.updateSettings(pageId, { db_schema: next })
    setSchema(p.db_schema ?? next)
    setColumnsOpen(false)
  }

  const saveTitle = async (id: number, title: string) => {
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, title } : r)))
    try {
      await api.updatePage(id, { title })
    } catch (e) {
      alert((e as Error).message)
    }
  }

  const saveProperty = async (row: RowSummary, colId: string, value: unknown) => {
    const properties = { ...row.properties, [colId]: value }
    setRows((rs) => rs.map((r) => (r.id === row.id ? { ...r, properties } : r)))
    try {
      await api.updateSettings(row.id, { properties })
    } catch (e) {
      alert((e as Error).message)
    }
  }

  const toggleSort = (col: string) =>
    setSort((s) => (s?.col === col ? (s.dir === 1 ? { col, dir: -1 } : null) : { col, dir: 1 }))

  const sorted = useMemo(() => {
    if (!sort) return rows
    const { col, dir } = sort
    const value = (r: RowSummary): string | number =>
      col === 'title' ? r.title : ((r.properties[col] as string | number | undefined) ?? '')
    return [...rows].sort((a, b) => {
      const va = value(a)
      const vb = value(b)
      if (va === vb) return 0
      return (va < vb ? -1 : 1) * dir
    })
  }, [rows, sort])

  if (loading) return <div className="loading">読み込み中…</div>

  return (
    <section className="database-table">
      <div className="database-toolbar">
        <h2 className="m-0">行（{rows.length}）</h2>
        {editable && (
          <div className="ms-auto d-flex gap-2">
            {schemaEditable && (
              <button type="button" className="btn btn-sm btn-outline-secondary" onClick={() => setColumnsOpen(true)}>
                ⚙ 列を編集
              </button>
            )}
            <button type="button" className="btn btn-sm btn-primary" onClick={addRow} disabled={adding}>
              ＋ 行を追加
            </button>
          </div>
        )}
      </div>
      {error && <p className="error">{error}</p>}
      <div className="table-responsive">
        <table className="table table-sm align-middle database-grid">
          <thead>
            <tr>
              <th>
                <button type="button" className="th-sort" onClick={() => toggleSort('title')}>
                  タイトル{sort?.col === 'title' ? (sort.dir === 1 ? ' ▲' : ' ▼') : ''}
                </button>
              </th>
              {schema.map((c) => (
                <th key={c.id}>
                  <button type="button" className="th-sort" onClick={() => toggleSort(c.id)}>
                    {c.name}
                    {sort?.col === c.id ? (sort.dir === 1 ? ' ▲' : ' ▼') : ''}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.map((row) => (
              <tr key={row.id}>
                <td className="database-row-title">
                  <span className="tree-icon">{row.icon || '📄'}</span>
                  {row.is_locked && (
                    <span className="me-1" title="ロック中（編集できるのは作った人と管理者だけ）">
                      🔐
                    </span>
                  )}
                  {editable && row.can_edit ? (
                    <input
                      className="form-control form-control-sm"
                      defaultValue={row.title}
                      placeholder="無題"
                      onBlur={(e) => e.target.value !== row.title && saveTitle(row.id, e.target.value)}
                    />
                  ) : (
                    <span>{displayTitle(row.title)}</span>
                  )}
                  <a
                    href={pagePath(row.id)}
                    className="database-row-open"
                    title="このページを開く"
                    onClick={(e) => (e.preventDefault(), navigate(pagePath(row.id)))}
                  >
                    ↗
                  </a>
                </td>
                {schema.map((c) => (
                  <td key={c.id}>
                    <Cell
                      column={c}
                      value={row.properties[c.id]}
                      editable={editable && row.can_edit}
                      onChange={(v) => saveProperty(row, c.id, v)}
                    />
                  </td>
                ))}
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={schema.length + 1} className="muted text-center py-4">
                  まだ行がありません
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {columnsOpen && (
        <DatabaseColumnsDialog schema={schema} onSave={saveColumns} onClose={() => setColumnsOpen(false)} />
      )}
    </section>
  )
}

function Cell({
  column,
  value,
  editable,
  onChange,
}: {
  column: DbColumn
  value: unknown
  editable: boolean
  onChange: (v: unknown) => void
}) {
  if (!editable) {
    if (column.kind === 'checkbox') return <span>{value ? '✓' : ''}</span>
    if (column.kind === 'url' && typeof value === 'string' && value) {
      const href = safeUrl(value, false)
      return href ? (
        <a href={href} target="_blank" rel="noopener noreferrer nofollow">
          {value}
        </a>
      ) : (
        <span>{value}</span>
      )
    }
    return <span>{typeof value === 'string' || typeof value === 'number' ? value : ''}</span>
  }

  switch (column.kind) {
    case 'checkbox':
      return (
        <input
          type="checkbox"
          className="form-check-input"
          checked={!!value}
          onChange={(e) => onChange(e.target.checked)}
        />
      )
    case 'number':
      return (
        <input
          type="number"
          className="form-control form-control-sm"
          defaultValue={typeof value === 'number' ? value : ''}
          onBlur={(e) => onChange(e.target.value === '' ? null : Number(e.target.value))}
        />
      )
    case 'date':
      return (
        <input
          type="date"
          className="form-control form-control-sm"
          defaultValue={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        />
      )
    case 'select':
      return (
        <select
          className="form-select form-select-sm"
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="" />
          {(column.options ?? []).map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      )
    case 'url':
      return (
        <input
          type="text"
          className="form-control form-control-sm"
          defaultValue={typeof value === 'string' ? value : ''}
          onBlur={(e) => e.target.value !== value && onChange(e.target.value)}
        />
      )
    default:
      return (
        <input
          type="text"
          className="form-control form-control-sm"
          defaultValue={typeof value === 'string' ? value : ''}
          onBlur={(e) => e.target.value !== value && onChange(e.target.value)}
        />
      )
  }
}
