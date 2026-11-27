import { useEffect, useState } from 'react'
import { api, type User } from '../api'
import { useApp } from '../AppContext'

const emptyForm = { username: '', display_name: '', password: '', is_admin: false }

export default function UsersAdmin() {
  const { me } = useApp()
  const [users, setUsers] = useState<User[]>([])
  const [form, setForm] = useState(emptyForm)
  const [error, setError] = useState<string | null>(null)

  const load = () =>
    api
      .listUsers()
      .then(setUsers)
      .catch((e) => setError(e.message))
  useEffect(() => {
    load()
  }, [])

  const run = async (fn: () => Promise<unknown>) => {
    setError(null)
    try {
      await fn()
      await load()
    } catch (e) {
      setError((e as Error).message)
    }
  }

  const create = (e: React.FormEvent) => {
    e.preventDefault()
    run(async () => {
      await api.createUser(form)
      setForm(emptyForm)
    })
  }

  const resetPassword = (u: User) => {
    const pw = prompt(`${u.display_name} の新しいパスワード（10文字以上）`)
    if (pw) run(() => api.updateUser(u.id, { password: pw }))
  }

  const resetTotp = (u: User) => {
    if (confirm(`${u.display_name} の2段階認証を解除しますか？（ログイン中の端末もログアウトされます）`))
      run(() => api.updateUser(u.id, { reset_totp: true }))
  }

  const remove = (u: User) => {
    if (confirm(`${u.display_name}（${u.username}）を削除しますか？`)) run(() => api.deleteUser(u.id))
  }

  return (
    <div className="vstack gap-4">
      {error && <p className="error">{error}</p>}
      <div className="card shadow-sm">
        <div className="table-responsive">
          <table className="table table-hover align-middle mb-0 user-table">
            <thead>
              <tr>
                <th>ユーザー名</th>
                <th>表示名</th>
                <th>管理者</th>
                <th>2段階認証</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <tr key={u.id}>
                  <td>{u.username}</td>
                  <td>{u.display_name}</td>
                  <td>
                    <input
                      className="form-check-input"
                      type="checkbox"
                      checked={u.is_admin}
                      disabled={u.id === me?.id}
                      onChange={(e) => run(() => api.updateUser(u.id, { is_admin: e.target.checked }))}
                    />
                  </td>
                  <td>{u.totp_enabled ? '✅ 有効' : <span className="muted">無効</span>}</td>
                  <td className="row-actions">
                    <button className="btn btn-sm btn-outline-secondary" onClick={() => resetPassword(u)}>
                      パスワード再設定
                    </button>
                    {u.totp_enabled && u.id !== me?.id && (
                      <button className="btn btn-sm btn-outline-secondary ms-1" onClick={() => resetTotp(u)}>
                        2段階認証を解除
                      </button>
                    )}
                    {u.id !== me?.id && (
                      <button className="btn btn-sm btn-outline-danger ms-1" onClick={() => remove(u)}>
                        削除
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <form className="card shadow-sm" onSubmit={create}>
        <div className="card-header brand-font">編集者を追加</div>
        <div className="card-body row g-3">
          <label className="form-label col-md-6 mb-0">
            ユーザー名（英数字と _ - .）
            <input
              className="form-control mt-1"
              value={form.username}
              onChange={(e) => setForm({ ...form, username: e.target.value })}
            />
          </label>
          <label className="form-label col-md-6 mb-0">
            表示名
            <input
              className="form-control mt-1"
              value={form.display_name}
              onChange={(e) => setForm({ ...form, display_name: e.target.value })}
            />
          </label>
          <label className="form-label col-md-6 mb-0">
            初期パスワード（10文字以上）
            <input
              className="form-control mt-1"
              type="password"
              value={form.password}
              autoComplete="new-password"
              onChange={(e) => setForm({ ...form, password: e.target.value })}
            />
          </label>
          <label className="form-check col-md-6 align-self-end mb-2 ms-2">
            <input
              className="form-check-input"
              type="checkbox"
              checked={form.is_admin}
              onChange={(e) => setForm({ ...form, is_admin: e.target.checked })}
            />
            管理者にする
          </label>
          <div className="col-12">
            <button
              type="submit"
              className="btn btn-primary"
              disabled={!form.username || !form.display_name || !form.password}
            >
              追加
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
