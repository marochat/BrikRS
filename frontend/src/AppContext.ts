import { createContext, useContext } from 'react'
import type { PageSummary, SiteSettings, Snippet, User } from './api'
import type { TocItem } from './toc'

export type AppState = {
  me: User | null
  setMe: (u: User | null) => void
  /** 編集モードか（ログインしていて、編集ボタンを押したときだけ true。既定は閲覧モード） */
  editing: boolean
  setEditing: (e: boolean) => void
  /** サイト全体の設定（ナビバーのロゴ） */
  site: SiteSettings
  setSite: (s: SiteSettings) => void
  pages: PageSummary[]
  setPages: (p: PageSummary[]) => void
  reloadPages: () => Promise<void>
  /** ブロックスニペット（「/」メニューで使うので、ブート時にまとめて読み込んでおく） */
  snippets: Snippet[]
  reloadSnippets: () => Promise<void>
  /** 検索を開く（q を渡すとその語で検索した状態で開く。ハッシュタグ用） */
  openSearch: (q?: string) => void
  /** 表示中のページの目次（サイドバーに出す） */
  toc: TocItem[]
  setToc: (t: TocItem[]) => void
  /** 承認待ちのゲストコメントの数（管理者だけ。それ以外は 0） */
  pendingComments: number
  refreshPendingComments: () => void
  /** タイトル・アイコンの変更をツリーへ即時反映する */
  patchPage: (id: number, patch: Partial<PageSummary>) => void
}

export const AppContext = createContext<AppState | null>(null)

export function useApp(): AppState {
  const ctx = useContext(AppContext)
  if (!ctx) throw new Error('AppContext is missing')
  return ctx
}

/** parent_id ごとの子リスト（sort_order 順） */
export function childrenMap(pages: PageSummary[]): Map<number | null, PageSummary[]> {
  const map = new Map<number | null, PageSummary[]>()
  for (const p of pages) {
    const list = map.get(p.parent_id) ?? []
    list.push(p)
    map.set(p.parent_id, list)
  }
  for (const list of map.values()) list.sort((a, b) => a.sort_order - b.sort_order || a.id - b.id)
  return map
}

/** ルートから id までの祖先リスト（自身を含む） */
export function ancestors(pages: PageSummary[], id: number): PageSummary[] {
  const byId = new Map(pages.map((p) => [p.id, p]))
  const out: PageSummary[] = []
  let cur = byId.get(id)
  while (cur && out.length < 100) {
    out.unshift(cur)
    cur = cur.parent_id === null ? undefined : byId.get(cur.parent_id)
  }
  return out
}

export const displayTitle = (t: string) => t.trim() || '無題'
