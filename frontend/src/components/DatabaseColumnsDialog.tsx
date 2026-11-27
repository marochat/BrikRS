import { useState } from 'react'
import type { DbColumn, DbColumnKind } from '../api'
import Modal from './Modal'

const KIND_LABELS: Record<DbColumnKind, string> = {
  text: 'テキスト',
  number: '数値',
  select: '選択',
  date: '日付',
  checkbox: 'チェックボックス',
  url: 'URL',
}

const newId = () => (crypto as { randomUUID?: () => string }).randomUUID?.() ?? `col_${Date.now()}_${Math.random()}`

type Props = {
  schema: DbColumn[]
  onSave: (next: DbColumn[]) => Promise<void> | void
  onClose: () => void
}

/** データベースページの列（プロパティ）の追加・削除・名称変更・型変更・選択肢編集 */
export default function DatabaseColumnsDialog({ schema, onSave, onClose }: Props) {
  const [cols, setCols] = useState<DbColumn[]>(schema.length ? schema : [])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const update = (id: string, patch: Partial<DbColumn>) =>
    setCols((cs) => cs.map((c) => (c.id === id ? { ...c, ...patch } : c)))

  const addColumn = () => setCols((cs) => [...cs, { id: newId(), name: '', kind: 'text' }])
  const removeColumn = (id: string) => setCols((cs) => cs.filter((c) => c.id !== id))

  const addOption = (id: string) =>
    update(id, { options: [...(cols.find((c) => c.id === id)?.options ?? []), ''] })
  const setOption = (id: string, i: number, v: string) => {
    const col = cols.find((c) => c.id === id)
    if (!col) return
    const options = [...(col.options ?? [])]
    options[i] = v
    update(id, { options })
  }
  const removeOption = (id: string, i: number) => {
    const col = cols.find((c) => c.id === id)
    if (!col) return
    update(id, { options: (col.options ?? []).filter((_, j) => j !== i) })
  }

  const save = async () => {
    const names = cols.map((c) => c.name.trim())
    if (names.some((n) => !n)) {
      setError('列の名前を入力してください')
      return
    }
    if (new Set(names).size !== names.length) {
      setError('列の名前が重複しています')
      return
    }
    setSaving(true)
    setError(null)
    try {
      await onSave(
        cols.map((c) => ({
          ...c,
          name: c.name.trim(),
          options: c.kind === 'select' ? (c.options ?? []).map((o) => o.trim()).filter(Boolean) : undefined,
        })),
      )
    } catch (e) {
      setError((e as Error).message)
      setSaving(false)
    }
  }

  return (
    <Modal onClose={onClose} dialogClassName="modal-dialog-centered modal-lg">
      <div className="modal-header">
        <h5 className="modal-title">列を編集</h5>
        <button type="button" className="btn-close" onClick={onClose} aria-label="閉じる" />
      </div>
      <div className="modal-body">
        {error && <p className="error">{error}</p>}
        {cols.map((c) => (
          <div key={c.id} className="database-column-row card mb-2 p-2">
            <div className="d-flex gap-2 align-items-center">
              <input
                className="form-control form-control-sm"
                placeholder="列の名前"
                value={c.name}
                onChange={(e) => update(c.id, { name: e.target.value })}
              />
              <select
                className="form-select form-select-sm w-auto"
                value={c.kind}
                onChange={(e) => update(c.id, { kind: e.target.value as DbColumnKind })}
              >
                {Object.entries(KIND_LABELS).map(([k, label]) => (
                  <option key={k} value={k}>
                    {label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="btn btn-sm btn-outline-danger ms-auto"
                onClick={() => removeColumn(c.id)}
              >
                削除
              </button>
            </div>
            {c.kind === 'select' && (
              <div className="mt-2 ps-2">
                {(c.options ?? []).map((o, i) => (
                  <div key={i} className="d-flex gap-2 align-items-center mb-1">
                    <input
                      className="form-control form-control-sm"
                      placeholder="選択肢"
                      value={o}
                      onChange={(e) => setOption(c.id, i, e.target.value)}
                    />
                    <button
                      type="button"
                      className="btn btn-sm btn-outline-secondary"
                      onClick={() => removeOption(c.id, i)}
                    >
                      ×
                    </button>
                  </div>
                ))}
                <button type="button" className="btn btn-sm btn-link px-0" onClick={() => addOption(c.id)}>
                  ＋ 選択肢を追加
                </button>
              </div>
            )}
          </div>
        ))}
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={addColumn}>
          ＋ 列を追加
        </button>
      </div>
      <div className="modal-footer">
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={onClose}>
          キャンセル
        </button>
        <button type="button" className="btn btn-sm btn-primary" onClick={save} disabled={saving}>
          保存
        </button>
      </div>
    </Modal>
  )
}
