import { useEffect, useState } from 'react'
import { api, type Group, type PageSummary, type User, type Visibility } from './api'

/** 公開範囲の組（このページ自身の設定） */
export type Access = { visibility: Visibility; group_ids: number[] }

export const PUBLIC_ACCESS: Access = { visibility: 'public', group_ids: [] }

// グループの一覧はログイン中に何度も使うので、1回読んだら使い回す（管理画面で変えたら invalidateGroups）
let cache: Promise<Group[]> | null = null
const listeners = new Set<() => void>()

export function loadGroups(): Promise<Group[]> {
  cache ??= api.listGroups().catch((e) => {
    cache = null
    throw e
  })
  return cache
}

export function invalidateGroups() {
  cache = null
  listeners.forEach((f) => f())
}

/** グループの一覧（ログインしていなければ空） */
export function useGroups(me: User | null): Group[] {
  const [groups, setGroups] = useState<Group[]>([])
  const [rev, setRev] = useState(0)
  useEffect(() => {
    const f = () => setRev((r) => r + 1)
    listeners.add(f)
    return () => {
      listeners.delete(f)
    }
  }, [])
  useEffect(() => {
    if (!me) return
    let alive = true
    loadGroups()
      .then((g) => alive && setGroups(g))
      .catch(() => alive && setGroups([]))
    return () => {
      alive = false
    }
  }, [me, rev])
  return me ? groups : NO_GROUPS
}

const NO_GROUPS: Group[] = []

/** 下書き・公開範囲を変えられるか（作った人と管理者） */
export const canManage = (me: User | null, createdById: number | null) =>
  !!me && (me.is_admin || (createdById !== null && createdById === me.id))

/** ロック中のページを自分が編集できないか（ロックを無視できるのは、作った人と管理者） */
export const lockedForMe = (me: User | null, p: { is_locked: boolean; created_by_id: number | null }) =>
  p.is_locked && !canManage(me, p.created_by_id)

/** 「グループ「A」「B」の人と管理者だけ」のような説明 */
export function accessLabel(a: Access, groups: Group[]): string {
  switch (a.visibility) {
    case 'public':
      return '誰でも見られます'
    case 'members':
      return 'ログインしている人だけ'
    case 'groups': {
      const names = a.group_ids.map((id) => groups.find((g) => g.id === id)?.name ?? '（削除されたグループ）')
      return names.length ? `${names.map((n) => `「${n}」`).join('')}の人と管理者だけ` : '管理者だけ'
    }
  }
}

/**
 * start とその祖先のうち、下書きか公開以外の範囲のページの id（サーバーの restricting_ancestors と同じ）。
 * ページを start の下に置いたとき、そのページに効く祖先の制限
 */
export function restrictingAncestors(pages: PageSummary[], start: number | null): Set<number> {
  const byId = new Map(pages.map((p) => [p.id, p]))
  const out = new Set<number>()
  let cur = start
  for (let steps = 0; cur !== null && steps <= pages.length; steps++) {
    const p = byId.get(cur)
    if (!p) break
    if (p.is_draft || p.visibility !== 'public') out.add(p.id)
    cur = p.parent_id
  }
  return out
}

/** サイドバーなどの小さな印 */
export function accessBadge(p: Pick<PageSummary, 'visibility'>): string | null {
  return p.visibility === 'members' ? '登録者' : p.visibility === 'groups' ? 'グループ' : null
}

