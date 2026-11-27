import { useSyncExternalStore } from 'react'

// 画面数が少ないので、ルーターライブラリを入れずに History API だけで済ませる

export type Route =
  | { name: 'home' }
  | { name: 'page'; id: number }
  | { name: 'login' }
  | { name: 'users' }
  | { name: 'account' }
  | { name: 'trash' }
  | { name: 'recent' }
  | { name: 'notFound' }

export function parseRoute(path: string): Route {
  if (path === '/' || path === '') return { name: 'home' }
  const m = path.match(/^\/p\/(\d+)\/?$/)
  if (m) return { name: 'page', id: Number(m[1]) }
  if (path === '/login') return { name: 'login' }
  if (path === '/admin/users') return { name: 'users' }
  if (path === '/account') return { name: 'account' }
  if (path === '/trash') return { name: 'trash' }
  if (path === '/recent') return { name: 'recent' }
  return { name: 'notFound' }
}

const listeners = new Set<() => void>()

function subscribe(cb: () => void) {
  listeners.add(cb)
  window.addEventListener('popstate', cb)
  return () => {
    listeners.delete(cb)
    window.removeEventListener('popstate', cb)
  }
}

export function navigate(path: string, replace = false) {
  if (path === location.pathname) return
  if (replace) history.replaceState(null, '', path)
  else history.pushState(null, '', path)
  listeners.forEach((l) => l())
}

export function useRoute(): Route {
  const path = useSyncExternalStore(subscribe, () => location.pathname)
  return parseRoute(path)
}

export const pagePath = (id: number) => `/p/${id}`
