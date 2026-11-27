/**
 * 保存待ちの変更を、ページの外（ナビバーのログアウトなど）からまとめて送るための登録簿。
 * ページの編集画面が自分の flush を登録しておき、ログアウトの前に flushAll() で送り切る。
 */
const flushers = new Set<() => Promise<unknown>>()

/** flush を登録する。戻り値を呼ぶと登録を外す（useEffect の後始末にそのまま使える） */
export function registerFlush(f: () => Promise<unknown>): () => void {
  flushers.add(f)
  return () => {
    flushers.delete(f)
  }
}

/** 登録されている保存待ちをすべて送る（失敗しても止めない） */
export async function flushAll(): Promise<void> {
  await Promise.allSettled([...flushers].map((f) => f()))
}
