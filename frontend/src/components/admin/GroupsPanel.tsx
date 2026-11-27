import { useEffect, useState, type FormEvent } from 'react'
import { api, type Group, type User } from '../../api'
import { invalidateGroups } from '../../access'

/**
 * 管理画面の「グループ」タブ：グループの作成・名前の変更・削除と、所属するユーザーの出し入れ。
 * 1人が複数のグループに入れる。新しいユーザーはどのグループにも入っていない。管理者はグループに関係なく全部見られる
 */
export default function GroupsPanel() {
  const [groups, setGroups] = useState<Group[]>([])
  const [users, setUsers] = useState<User[]>([])
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)

  const load = () =>
    Promise.all([api.listGroups(), api.listUsers()])
      .then(([g, u]) => {
        setGroups(g)
        setUsers(u)
      })
      .catch((e) => setError(e.message))
  useEffect(() => {
    load()
  }, [])

  const run = async (fn: () => Promise<unknown>) => {
    setError(null)
    try {
      await fn()
    } catch (e) {
      setError((e as Error).message)
    }
    invalidateGroups()
    await load()
  }

  const create = (e: FormEvent) => {
    e.preventDefault()
    run(async () => {
      await api.createGroup(name)
      setName('')
    })
  }

  const rename = (g: Group) => {
    const n = prompt('グループの名前', g.name)
    if (n && n !== g.name) run(() => api.updateGroup(g.id, { name: n }))
  }

  const remove = (g: Group) => {
    const pages = g.page_count ?? 0
    const note = pages
      ? `\n\nこのグループを公開範囲に選んでいるページが ${pages} 件あります。このグループだけに公開しているページは、管理者だけが見られるようになります。`
      : ''
    if (confirm(`グループ「${g.name}」を削除しますか？${note}`)) run(() => api.deleteGroup(g.id))
  }

  const toggleMember = (g: Group, userId: number) => {
    const ids = g.user_ids ?? []
    const next = ids.includes(userId) ? ids.filter((i) => i !== userId) : [...ids, userId]
    run(() => api.updateGroup(g.id, { user_ids: next }))
  }

  return (
    <div className="vstack gap-4">
      {error && <p className="error">{error}</p>}
      <p className="muted small m-0">
        ページの「…」→「公開範囲・下書き…」で「グループ」を選ぶと、選んだグループの人と管理者だけが見られます（子ページも含む）。
        管理者は特別なグループの扱いで、どのグループに入っていなくても全部見られます。
      </p>
      {groups.length === 0 && <p className="muted">まだグループはありません。</p>}
      {groups.length > 0 && (
        <div className="card shadow-sm">
          <div className="table-responsive">
            <table className="table align-middle mb-0 user-table group-table">
              <thead>
                <tr>
                  <th>ユーザー</th>
                  {groups.map((g) => (
                    <th key={g.id} className="text-center">
                      <div className="text-nowrap">{g.name}</div>
                      <div className="muted small fw-normal">ページ {g.page_count ?? 0} 件</div>
                      <div className="text-nowrap">
                        <button className="btn btn-sm btn-link p-0" onClick={() => rename(g)}>
                          名前
                        </button>
                        <button className="btn btn-sm btn-link text-danger p-0 ms-2" onClick={() => remove(g)}>
                          削除
                        </button>
                      </div>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {users.map((u) => (
                  <tr key={u.id}>
                    <td>
                      {u.display_name} <span className="muted small">{u.username}</span>
                      {u.is_admin && <span className="badge text-bg-light border ms-1">管理者</span>}
                    </td>
                    {groups.map((g) => (
                      <td key={g.id} className="text-center">
                        <input
                          className="form-check-input"
                          type="checkbox"
                          aria-label={`${u.display_name} を ${g.name} に入れる`}
                          checked={g.user_ids?.includes(u.id) ?? false}
                          onChange={() => toggleMember(g, u.id)}
                        />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
      <form className="card shadow-sm" onSubmit={create}>
        <div className="card-header brand-font">グループを追加</div>
        <div className="card-body d-flex gap-2">
          <input
            className="form-control"
            placeholder="グループの名前"
            maxLength={64}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <button type="submit" className="btn btn-primary text-nowrap" disabled={!name.trim()}>
            追加
          </button>
        </div>
      </form>
    </div>
  )
}
