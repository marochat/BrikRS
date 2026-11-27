import { useSyncExternalStore } from 'react'

/*
 * カラーテーマ。選んだ値（auto / light / dark）を localStorage に覚え、
 * 実際の色（light / dark）を <html data-bs-theme> に入れる。Bootstrap とアプリの CSS はこの属性で色を変える。
 * ページを開いた直後の色は public/theme-init.js が React より先に決める（一瞬違う色が見えないように）。
 */

export type ThemePref = 'auto' | 'light' | 'dark'
export type Theme = 'light' | 'dark'

export const THEME_OPTIONS: { value: ThemePref; label: string }[] = [
  { value: 'auto', label: 'OS の設定に従う' },
  { value: 'light', label: 'ライト' },
  { value: 'dark', label: 'ダーク' },
]

// theme-init.js と同じキー
const KEY = 'brikrs.theme'
const darkQuery = matchMedia('(prefers-color-scheme: dark)')
const listeners = new Set<() => void>()

function readPref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY)
    return v === 'light' || v === 'dark' ? v : 'auto'
  } catch {
    return 'auto'
  }
}

let pref: ThemePref = readPref()

const resolve = (): Theme => (pref === 'auto' ? (darkQuery.matches ? 'dark' : 'light') : pref)

function apply() {
  const t = resolve()
  if (document.documentElement.dataset.bsTheme !== t) document.documentElement.dataset.bsTheme = t
  listeners.forEach((l) => l())
}

// 「OS の設定に従う」のときは OS 側の切り替えにも追従する
darkQuery.addEventListener('change', () => pref === 'auto' && apply())
apply()

export function getThemePref(): ThemePref {
  return pref
}

export function setThemePref(p: ThemePref) {
  pref = p
  try {
    localStorage.setItem(KEY, p)
  } catch {
    /* 保存できなくても、今開いている画面には反映する */
  }
  apply()
}

const subscribe = (cb: () => void) => {
  listeners.add(cb)
  return () => listeners.delete(cb)
}

/** 実際に使われている色（light / dark） */
export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, resolve)
}

/** 選んでいる設定（auto / light / dark） */
export function useThemePref(): ThemePref {
  return useSyncExternalStore(subscribe, getThemePref)
}
