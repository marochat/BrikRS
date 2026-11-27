import { useEffect } from 'react'
import type { BlockNoteEditor } from '@blocknote/core'

/*
 * IME の調査用ログ（変換中の Backspace でひらがなのまま確定してしまう問題。TODO_REVIEW.md 参照）。
 * ブラウザのコンソールで localStorage.setItem('brikrs.imeDebug', '1') としてから読み込み直したときだけ動く。
 * イベント・トランザクション・DOM の変化を時刻つきで console に出し、window.__imeLog にも貯める
 * （copy(JSON.stringify(__imeLog)) でまとめて写せる）
 */

const enabled = () => {
  try {
    return localStorage.getItem('brikrs.imeDebug') === '1'
  } catch {
    return false
  }
}

type Entry = { t: number; kind: string; [k: string]: unknown }

const EVENTS = ['keydown', 'keyup', 'beforeinput', 'input', 'compositionstart', 'compositionupdate', 'compositionend'] as const

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function useImeDebug(editor: BlockNoteEditor<any, any, any>) {
  useEffect(() => {
    if (!enabled()) return
    const w = window as unknown as { __imeLog?: Entry[] }
    const log = (w.__imeLog ??= [])
    const t0 = performance.now()
    const view = () => editor.prosemirrorView
    const push = (kind: string, data: Record<string, unknown>) => {
      const e: Entry = { t: Math.round(performance.now() - t0), kind, composing: view()?.composing, ...data }
      log.push(e)
      console.log('[ime]', e.t, kind, data, 'composing=', e.composing)
    }

    // イベントは document の capture 段階で（エディタの中のものだけ）。どの処理より先に見える
    const inEditor = (e: Event) => e.target instanceof Node && !!editor.domElement?.contains(e.target)
    const onEvent = (e: Event) => {
      if (!inEditor(e)) return
      const k = e as KeyboardEvent & InputEvent & CompositionEvent
      push(e.type, {
        key: k.key,
        keyCode: k.keyCode,
        isComposing: k.isComposing,
        inputType: k.inputType,
        data: k.data,
        defaultPrevented: e.defaultPrevented,
      })
    }
    // 他の処理が preventDefault したかは bubble の最後で分かる
    const onEventAfter = (e: Event) => {
      if (inEditor(e) && e.defaultPrevented) push(`${e.type}:prevented`, {})
    }
    for (const t of EVENTS) {
      document.addEventListener(t, onEvent, true)
      window.addEventListener(t, onEventAfter)
    }

    // トランザクション（本文や選択の変更、どのプラグインの meta が付いているか）
    const onTr = ({ transaction: tr }: { transaction: import('prosemirror-state').Transaction }) =>
      push('transaction', {
        docChanged: tr.docChanged,
        steps: tr.steps.map((s) => s.toJSON()),
        meta: Object.keys((tr as unknown as { meta: Record<string, unknown> }).meta ?? {}),
        selection: [tr.selection.from, tr.selection.to],
        // 本文の変わらないトランザクションは、誰が送ったかを呼び出し履歴で見る
        stack: tr.docChanged ? undefined : new Error().stack?.split('\n').slice(2, 25).map((l) => l.trim()),
      })
    editor._tiptapEditor.on('transaction', onTr)

    // DOM の変化。変換中の文字のテキストノードが消された・入れ替えられた瞬間を見る
    let mo: MutationObserver | undefined
    const startObserver = () => {
      const dom = editor.domElement
      if (!dom || mo) return
      mo = new MutationObserver((ms) => {
        for (const m of ms) {
          if (m.type === 'characterData') {
            push('dom:text', { old: m.oldValue, now: m.target.textContent })
          } else {
            const names = (ns: NodeList) => Array.from(ns, (n) => n.nodeName + (n.nodeType === 3 ? `(${n.textContent})` : ''))
            push('dom:children', {
              target: (m.target as Element).nodeName + '.' + ((m.target as Element).className || ''),
              removed: names(m.removedNodes),
              added: names(m.addedNodes),
            })
          }
        }
      })
      mo.observe(dom, { subtree: true, childList: true, characterData: true, characterDataOldValue: true })
    }
    startObserver()
    // エディタの DOM がまだ無いとき（初回の描画前）は少し待つ
    const timer = window.setTimeout(startObserver, 500)
    console.log('[ime] 調査用ログを開始しました（止めるには localStorage.removeItem("brikrs.imeDebug")）')

    return () => {
      for (const t of EVENTS) {
        document.removeEventListener(t, onEvent, true)
        window.removeEventListener(t, onEventAfter)
      }
      editor._tiptapEditor.off('transaction', onTr)
      window.clearTimeout(timer)
      mo?.disconnect()
    }
  }, [editor])
}
