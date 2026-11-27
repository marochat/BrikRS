/** URL のホスト名（カードの右下などに出す）。URL として読めなければそのまま */
export function linkHost(url: string): string {
  try {
    return new URL(url).host
  } catch {
    return url
  }
}
