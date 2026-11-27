import { createContext, useCallback, useMemo, useState } from 'react'
import type { InteractionStats, Interactions } from './api'

// タイムラインの投稿へのリアクション・ゲストコメントの受け付けと件数を、一覧やスレッドの応答から配る入れ物

const EMPTY: Interactions = { comments: false, reactions: false, captcha: false, items: {} }

export type InteractionsStore = {
  inter: Interactions
  /** 一覧の続きを読み込んだとき：受け付けの設定は新しいものに、件数は足し合わせる */
  merge: (i: Interactions) => void
  /** 1件分の件数を差し替える（無ければ消す） */
  setStats: (id: number, s: InteractionStats | undefined) => void
}

export const InteractionsContext = createContext<InteractionsStore | null>(null)

/** 件数を持つ入れ物。InteractionsContext.Provider に渡す */
export function useInteractionsStore(initial?: Interactions): InteractionsStore {
  const [inter, setInter] = useState<Interactions>(initial ?? EMPTY)
  const merge = useCallback(
    (i: Interactions) => setInter((cur) => ({ ...i, items: { ...cur.items, ...i.items } })),
    [],
  )
  const setStats = useCallback(
    (id: number, s: InteractionStats | undefined) =>
      setInter((cur) => {
        const items = { ...cur.items }
        if (s) items[id] = s
        else delete items[id]
        return { ...cur, items }
      }),
    [],
  )
  return useMemo(() => ({ inter, merge, setStats }), [inter, merge, setStats])
}
