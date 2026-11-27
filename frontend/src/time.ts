/** 「〇分前」のような相対表示（1週間を超えたら日付） */
export function ago(iso: string): string {
  const sec = (Date.now() - new Date(iso).getTime()) / 1000
  if (sec < 60) return 'たった今'
  if (sec < 3600) return `${Math.floor(sec / 60)}分前`
  if (sec < 86400) return `${Math.floor(sec / 3600)}時間前`
  if (sec < 7 * 86400) return `${Math.floor(sec / 86400)}日前`
  return new Date(iso).toLocaleDateString('ja-JP')
}

/** start〜end の毎日を並べる（記録の無い日も 0 として描くため） */
export function eachDay(start: string, end: string): string[] {
  const out: string[] = []
  const d = new Date(`${start}T00:00:00Z`)
  const last = new Date(`${end}T00:00:00Z`)
  while (d <= last && out.length < 400) {
    out.push(d.toISOString().slice(0, 10))
    d.setUTCDate(d.getUTCDate() + 1)
  }
  return out
}
