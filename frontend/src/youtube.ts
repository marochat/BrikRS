/*
 * YouTube 埋め込み用。ブロックには動画 ID（11文字）だけを保存し、サムネイル・iframe の URL はここで組み立てる
 * （入力された URL をそのまま src に使わない）
 */

const VIDEO_ID = /^[A-Za-z0-9_-]{11}$/
const HOSTS = new Set(['youtube.com', 'www.youtube.com', 'm.youtube.com', 'music.youtube.com', 'www.youtube-nocookie.com'])

export const isVideoId = (s: string) => VIDEO_ID.test(s)

/** 動画の URL（watch・youtu.be・shorts・embed・live）から動画 ID を取り出す。ID そのものも受け付ける */
export function parseVideoId(input: string): string | null {
  const s = input.trim()
  if (isVideoId(s)) return s
  let u: URL
  try {
    u = new URL(/^[a-z][a-z0-9+.-]*:/i.test(s) ? s : `https://${s}`)
  } catch {
    return null
  }
  if (u.protocol !== 'https:' && u.protocol !== 'http:') return null
  let id: string | null | undefined
  if (u.hostname === 'youtu.be') {
    id = u.pathname.split('/')[1]
  } else if (HOSTS.has(u.hostname)) {
    const [, head, rest] = u.pathname.split('/')
    id = head === 'watch' ? u.searchParams.get('v') : ['shorts', 'embed', 'live', 'v'].includes(head) ? rest : null
  }
  return id && isVideoId(id) ? id : null
}

export const thumbnailUrl = (id: string) => `https://i.ytimg.com/vi/${id}/hqdefault.jpg`

export const watchUrl = (id: string) => `https://www.youtube.com/watch?v=${id}`

/** クッキーを使わない youtube-nocookie.com の埋め込み（CSP の frame-src で許可している） */
export const embedUrl = (id: string) => `https://www.youtube-nocookie.com/embed/${id}?autoplay=1`
