import { useEffect, useState } from 'react'

type KatexModule = typeof import('katex')

// KaTeX（JS 約80KB gzip）は数式を使わないページでは要らないので、実際に描画するときだけ読み込む
// （BlockView が BlockNote を読み込まないのと同じ理由）。一度読めば以後は同じ Promise を使い回す
let katexPromise: Promise<KatexModule> | null = null
function loadKatex(): Promise<KatexModule> {
  katexPromise ??= Promise.all([import('katex'), import('katex/dist/katex.min.css')]).then(([m]) => m)
  return katexPromise
}

/**
 * LaTeX を KaTeX で描画する。構文エラーは例外にせず、KaTeX が赤字のエラー表示を出す
 * （throwOnError: false）。編集画面・閲覧画面（BlockView）の両方から使う
 */
export function Katex({ formula, display }: { formula: string; display: boolean }) {
  const [html, setHtml] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    loadKatex().then(({ default: katex }) => {
      if (active) setHtml(katex.renderToString(formula, { displayMode: display, throwOnError: false, output: 'html' }))
    })
    return () => {
      active = false
    }
  }, [formula, display])

  // 読み込み中・失敗時は LaTeX をそのまま出す
  if (html === null) return <code>{formula}</code>
  return <span dangerouslySetInnerHTML={{ __html: html }} />
}
