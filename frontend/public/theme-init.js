// 画面を描く前にカラーテーマを決める（src/theme.ts と同じ判定）。
// CSP で inline script を禁止しているので、外部ファイルとして <head> で同期的に読み込む
;(function () {
  var pref = 'auto'
  try {
    pref = localStorage.getItem('brikrs.theme') || 'auto'
  } catch {
    /* 読めなければ OS の設定に従う */
  }
  var dark = pref === 'dark' || (pref !== 'light' && matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.setAttribute('data-bs-theme', dark ? 'dark' : 'light')
})()
