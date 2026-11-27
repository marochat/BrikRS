// アクセス解析用の匿名 ID。訪問者 ID は localStorage（端末ごとに続く）、
// セッション ID は sessionStorage（タブを閉じるまで）に置く。個人を特定する情報は含まない
function getOrCreate(storage: () => Storage, key: string): string {
  try {
    const s = storage()
    const existing = s.getItem(key)
    if (existing) return existing
    const created = crypto.randomUUID()
    s.setItem(key, created)
    return created
  } catch {
    // プライベートブラウズなどで保存できなければ、毎回新しい ID になる
    return crypto.randomUUID()
  }
}

export const getVisitorId = () => getOrCreate(() => localStorage, 'brikrs.visitor')
export const getSessionId = () => getOrCreate(() => sessionStorage, 'brikrs.session')
