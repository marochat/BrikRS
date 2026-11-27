/*
 * Google マップの埋め込み用。サーバーの gmaps.rs と同じ規則。
 * ブロックには pb（埋め込みコード）・mid（マイマップ）・q＋ll＋zoom（場所の URL）のどれかだけを保存し、
 * iframe の URL はここで組み立てる（入力された URL や HTML をそのまま src に使わない）
 */

export type MapProps = { mapsPb: string; mapsMid: string; mapsQ: string; mapsLl: string; mapsZoom: string }

export const emptyMap: MapProps = { mapsPb: '', mapsMid: '', mapsQ: '', mapsLl: '', mapsZoom: '' }

const PB = /^![A-Za-z0-9!._~*%:+,-]*$/
const MID = /^[A-Za-z0-9_.-]{10,100}$/
const NUM = /^-?[0-9.]{1,20}$/
const HOSTS = new Set(['google.com', 'www.google.com', 'maps.google.com', 'google.co.jp', 'www.google.co.jp', 'maps.google.co.jp'])

export const isPb = (s: string) => s.length <= 4096 && PB.test(s)
export const isMid = (s: string) => MID.test(s)
// eslint-disable-next-line no-control-regex
export const isQ = (s: string) => s.trim() !== '' && [...s].length <= 200 && !/[\u0000-\u001f\u007f-\u009f]/.test(s)
export function isLl(s: string) {
  const [lat, lng, ...more] = s.split(',')
  const ok = (t: string | undefined, max: number) => !!t && NUM.test(t) && Number.isFinite(Number(t)) && Math.abs(Number(t)) <= max
  return more.length === 0 && ok(lat, 90) && ok(lng, 180)
}
export const isZoom = (s: string) => /^[0-9]{1,2}$/.test(s) && Number(s) >= 1 && Number(s) <= 21

/** `@35.681,139.767,17z` → 座標とズーム（`1234m` のようなメートル指定はズームなし） */
function parseAt(seg: string): { ll: string; zoom: string } | null {
  if (!seg.startsWith('@')) return null
  const [lat, lng, z] = seg.slice(1).split(',')
  const ll = `${lat},${lng}`
  if (!isLl(ll)) return null
  const zoom = z?.endsWith('z') ? z.slice(0, -1).split('.')[0] : ''
  return { ll, zoom: isZoom(zoom) ? zoom : '' }
}

const decodeSegment = (seg: string) => {
  try {
    return decodeURIComponent(seg.replace(/\+/g, ' '))
  } catch {
    return ''
  }
}

/** 埋め込みコード（`<iframe src="…">`）なら src を取り出す。それ以外はそのまま */
function srcOf(input: string): string {
  const m = /<iframe\b[^>]*\bsrc\s*=\s*(["'])(.*?)\1/is.exec(input)
  return (m ? m[2].replace(/&amp;/g, '&') : input).trim()
}

/** 地図の URL・埋め込みコードを props にする。読み取れなければ null */
export function parseMap(input: string): MapProps | null {
  let u: URL
  try {
    u = new URL(srcOf(input))
  } catch {
    return null
  }
  if ((u.protocol !== 'https:' && u.protocol !== 'http:') || !HOSTS.has(u.hostname)) return null
  const segs = u.pathname.split('/').filter(Boolean)
  let rest: string[]
  if (segs[0] === 'maps') rest = segs.slice(1)
  else if (u.hostname.startsWith('maps.')) rest = segs
  else return null
  const m: MapProps = { ...emptyMap }
  if (rest.length === 1 && rest[0] === 'embed') {
    // pb はデコードせずに持つ
    const pb = u.search.slice(1).split('&').find((kv) => kv.startsWith('pb='))?.slice(3)
    if (!pb || !isPb(pb)) return null
    m.mapsPb = pb
  } else if (rest[0] === 'd') {
    const mid = u.searchParams.get('mid')
    if (!mid || !isMid(mid)) return null
    m.mapsMid = mid
  } else if (rest[0] === 'dir' || rest[0] === 'embed') {
    return null
  } else {
    const at = rest.map(parseAt).find(Boolean)
    if (at) {
      m.mapsLl = at.ll
      m.mapsZoom = at.zoom
    }
    const name = (rest[0] === 'place' || rest[0] === 'search') && rest[1] && !rest[1].startsWith('@') ? decodeSegment(rest[1]) : null
    const q = [name, u.searchParams.get('q'), u.searchParams.get('query')].find((s) => s != null && isQ(s))
    if (q) m.mapsQ = q.trim()
    else if (m.mapsLl) m.mapsQ = m.mapsLl
    else return null
    const z = u.searchParams.get('z') ?? ''
    if (!m.mapsZoom && isZoom(z)) m.mapsZoom = z
    const ll = u.searchParams.get('ll') ?? ''
    if (!m.mapsLl && isLl(ll)) m.mapsLl = ll
  }
  return m
}

function queryUrl(p: MapProps, embed: boolean) {
  let s = `https://www.google.com/maps?q=${encodeURIComponent(p.mapsQ)}`
  if (isLl(p.mapsLl)) s += `&ll=${p.mapsLl}`
  if (isZoom(p.mapsZoom)) s += `&z=${p.mapsZoom}`
  return embed ? `${s}&output=embed` : s
}

/** iframe の URL（CSP の frame-src で www.google.com を許可している）。表示できる値が無ければ null */
export function embedUrl(p: MapProps): string | null {
  if (isPb(p.mapsPb)) return `https://www.google.com/maps/embed?pb=${p.mapsPb}`
  if (isMid(p.mapsMid)) return `https://www.google.com/maps/d/embed?mid=${p.mapsMid}`
  if (isQ(p.mapsQ)) return queryUrl(p, true)
  return null
}

/** 「Google マップで開く」の URL（埋め込みコードの地図は、地図の中に Google のリンクがあるので無し） */
export function openUrl(p: MapProps): string | null {
  if (isPb(p.mapsPb)) return null
  if (isMid(p.mapsMid)) return `https://www.google.com/maps/d/viewer?mid=${p.mapsMid}`
  if (isQ(p.mapsQ)) return queryUrl(p, false)
  return null
}

/** ブロックの props から地図の値を取り出す（型が違うものは空にする） */
export function mapProps(p: Record<string, unknown>): MapProps {
  const s = (v: unknown) => (typeof v === 'string' ? v : '')
  return { mapsPb: s(p.mapsPb), mapsMid: s(p.mapsMid), mapsQ: s(p.mapsQ), mapsLl: s(p.mapsLl), mapsZoom: s(p.mapsZoom) }
}
