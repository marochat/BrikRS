import type { Group, User, Visibility } from '../api'
import type { Access } from '../access'

/**
 * 公開範囲の選択（ラジオ＋グループのチェック）。「管理者」は特別なグループとして常に入っている扱いで、外せない。
 * 管理者でない人は、自分が入っているグループを1つ以上選ぶ必要がある（サーバーでも確かめる）
 */
export function AccessPicker({
  value,
  onChange,
  groups,
  me,
  idPrefix,
}: {
  value: Access
  onChange: (a: Access) => void
  groups: Group[]
  me: User
  idPrefix: string
}) {
  const set = (visibility: Visibility) => onChange({ ...value, visibility })
  const toggle = (id: number) =>
    onChange({
      ...value,
      group_ids: value.group_ids.includes(id) ? value.group_ids.filter((g) => g !== id) : [...value.group_ids, id],
    })
  const radio = (v: Visibility, label: string, note: string) => (
    <div className="form-check">
      <input
        className="form-check-input"
        type="radio"
        id={`${idPrefix}-${v}`}
        name={`${idPrefix}-visibility`}
        checked={value.visibility === v}
        onChange={() => set(v)}
      />
      <label className="form-check-label" htmlFor={`${idPrefix}-${v}`}>
        {label} <span className="muted small">{note}</span>
      </label>
    </div>
  )
  return (
    <div className="vstack gap-1">
      {radio('public', '公開', '誰でも（ログインしていない人も）')}
      {radio('members', '登録ユーザー', 'ログインしている人だけ')}
      {radio('groups', 'グループ', '選んだグループの人と管理者だけ')}
      {value.visibility === 'groups' && (
        <div className="access-groups ms-4 vstack gap-1">
          <div className="form-check">
            <input className="form-check-input" type="checkbox" id={`${idPrefix}-admins`} checked disabled />
            <label className="form-check-label" htmlFor={`${idPrefix}-admins`}>
              管理者 <span className="muted small">（いつでも全部見られます）</span>
            </label>
          </div>
          {groups.map((g) => (
            <div className="form-check" key={g.id}>
              <input
                className="form-check-input"
                type="checkbox"
                id={`${idPrefix}-g${g.id}`}
                checked={value.group_ids.includes(g.id)}
                onChange={() => toggle(g.id)}
              />
              <label className="form-check-label" htmlFor={`${idPrefix}-g${g.id}`}>
                {g.name}
                {g.member && <span className="badge text-bg-light border ms-1">所属</span>}
              </label>
            </div>
          ))}
          {groups.length === 0 && (
            <p className="muted small m-0">
              グループはまだありません{me.is_admin ? '（管理画面の「グループ」で作れます）' : ''}
            </p>
          )}
          {!me.is_admin && !value.group_ids.some((id) => groups.find((g) => g.id === id)?.member) && (
            <p className="text-warning-emphasis small m-0">
              自分が入っているグループを1つ以上選んでください（自分も見られなくなるため）
            </p>
          )}
        </div>
      )}
    </div>
  )
}

/** 投稿欄用の小さな選択（公開 / 登録ユーザー / グループ1つ）。複数のグループは投稿後に「…」から */
export function AccessSelect({
  value,
  onChange,
  groups,
  me,
  disabled,
}: {
  value: Access
  onChange: (a: Access) => void
  groups: Group[]
  me: User
  disabled?: boolean
}) {
  // 管理者以外は、自分が入っているグループだけを出す（入っていないグループだけには公開できない）
  const choices = groups.filter((g) => me.is_admin || g.member)
  const key =
    value.visibility === 'groups' ? (value.group_ids.length === 1 ? `g${value.group_ids[0]}` : 'admins') : value.visibility
  const pick = (k: string) => {
    if (k === 'public' || k === 'members') onChange({ visibility: k, group_ids: [] })
    else if (k === 'admins') onChange({ visibility: 'groups', group_ids: [] })
    else onChange({ visibility: 'groups', group_ids: [Number(k.slice(1))] })
  }
  return (
    <select
      className="form-select form-select-sm w-auto"
      aria-label="公開範囲"
      title="公開範囲"
      value={key}
      disabled={disabled}
      onChange={(e) => pick(e.target.value)}
    >
      <option value="public">🌐 公開</option>
      <option value="members">👥 登録ユーザー</option>
      {me.is_admin && <option value="admins">🔒 管理者だけ</option>}
      {choices.map((g) => (
        <option key={g.id} value={`g${g.id}`}>
          🔒 {g.name}
        </option>
      ))}
    </select>
  )
}
