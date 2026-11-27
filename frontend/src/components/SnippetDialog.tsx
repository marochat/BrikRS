import { useState } from 'react'
import { BlockNoteView } from '@blocknote/mantine'
import '@blocknote/mantine/style.css'
import { api, type Snippet } from '../api'
import { useApp } from '../AppContext'
import { useTheme } from '../theme'
import { useBrikEditor } from './Editor'
import { BrikFormattingToolbar } from './ImageWrap'
import { BrikSideMenu } from './BrikSideMenu'
import IconPicker from './IconPicker'
import Modal from './Modal'

type Props = { onClose: () => void }

/** スニペットの管理（一覧・削除・新規作成・編集への入口） */
export default function SnippetDialog({ onClose }: Props) {
  const { snippets, reloadSnippets } = useApp()
  const [editing, setEditing] = useState<Snippet | 'new' | null>(null)

  const remove = async (id: number) => {
    if (!confirm('このスニペットを削除しますか？')) return
    try {
      await api.deleteSnippet(id)
      await reloadSnippets()
    } catch (e) {
      alert((e as Error).message)
    }
  }

  if (editing) {
    return (
      <SnippetForm
        snippet={editing === 'new' ? null : editing}
        onClose={() => setEditing(null)}
        onSaved={async () => {
          await reloadSnippets()
          setEditing(null)
        }}
      />
    )
  }

  return (
    <Modal onClose={onClose} dialogClassName="modal-dialog-centered modal-lg">
      <div className="modal-header">
        <h5 className="modal-title">スニペットを管理</h5>
        <button type="button" className="btn-close" onClick={onClose} aria-label="閉じる" />
      </div>
      <div className="modal-body">
        {snippets.length === 0 && <p className="muted">まだスニペットがありません。</p>}
        {snippets.length > 0 && (
          <ul className="list-group mb-3">
            {snippets.map((s) => (
              <li key={s.id} className="list-group-item d-flex align-items-center gap-2">
                <span className="tree-icon">{s.icon || '📎'}</span>
                <span className="flex-grow-1">{s.name}</span>
                <button type="button" className="btn btn-sm btn-outline-secondary" onClick={() => setEditing(s)}>
                  編集
                </button>
                <button type="button" className="btn btn-sm btn-outline-danger" onClick={() => remove(s.id)}>
                  削除
                </button>
              </li>
            ))}
          </ul>
        )}
        <button type="button" className="btn btn-sm btn-outline-secondary" onClick={() => setEditing('new')}>
          ＋ 新規作成
        </button>
      </div>
    </Modal>
  )
}

function SnippetForm({
  snippet,
  onClose,
  onSaved,
}: {
  snippet: Snippet | null
  onClose: () => void
  onSaved: () => void
}) {
  const theme = useTheme()
  const editor = useBrikEditor(snippet?.content ?? [])
  const [name, setName] = useState(snippet?.name ?? '')
  const [icon, setIcon] = useState(snippet?.icon ?? '')
  const [pickerOpen, setPickerOpen] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const save = async () => {
    if (!name.trim()) {
      setError('名前を入力してください')
      return
    }
    setSaving(true)
    setError(null)
    try {
      const content = editor.document
      if (snippet) await api.updateSnippet(snippet.id, { name, icon, content })
      else await api.createSnippet({ name, icon, content })
      onSaved()
    } catch (e) {
      setError((e as Error).message)
      setSaving(false)
    }
  }

  return (
    <Modal onClose={onClose} dialogClassName="modal-dialog-centered modal-lg">
      <div className="modal-header">
        <h5 className="modal-title">{snippet ? 'スニペットを編集' : '新しいスニペット'}</h5>
        <button type="button" className="btn-close" onClick={onClose} aria-label="閉じる" />
      </div>
      <div className="modal-body">
        {error && <p className="error">{error}</p>}
        <div className="d-flex gap-2 mb-2">
          <div className="position-relative">
            <button
              type="button"
              className="btn btn-outline-secondary btn-sm"
              onClick={() => setPickerOpen((o) => !o)}
            >
              {icon || '🙂'}
            </button>
            {pickerOpen && (
              <IconPicker
                hasIcon={!!icon}
                onPick={(i) => {
                  setIcon(i)
                  setPickerOpen(false)
                }}
                onClose={() => setPickerOpen(false)}
              />
            )}
          </div>
          <input
            className="form-control form-control-sm"
            placeholder="名前"
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={64}
          />
        </div>
        <div className="snippet-editor border rounded p-2">
          <BlockNoteView editor={editor} theme={theme} formattingToolbar={false} sideMenu={false}>
            <BrikFormattingToolbar />
            <BrikSideMenu />
          </BlockNoteView>
        </div>
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
