import { createExtension } from '@blocknote/core'
import { Plugin, type Transaction } from 'prosemirror-state'
import type { EditorView } from 'prosemirror-view'

/**
 * IME の変換中に Backspace を押すと、ひらがなのまま確定してしまう問題の対策（Chrome・Edge・Firefox）。
 *
 * 変換中の文字が Backspace で減ると、ProseMirror は DOM の変化を読み込む途中で「Backspace が押された」として
 * handleKeyDown を呼ぶ（prosemirror-view の readDOMChange）。BlockNote の Backspace の処理は
 * Tiptap の commands.first で、どの処理も当てはまらなくても中身の無いトランザクションを送ってしまう。
 * それを受けた ProseMirror が、IME の変化を読み込む前の古い状態で変換中のテキストノードを描き直すため、
 * ブラウザが変換を確定させる。
 *
 * そこで変換中は、本文・選択・meta のどれも変えないトランザクションを捨てる
 * （Tiptap は捨てられたトランザクションでは描き直さない）。
 */

/** 何も変えないトランザクション（meta は公開 API で列挙できないので内部の meta を見る） */
const isNoop = (tr: Transaction) =>
  !tr.docChanged &&
  !tr.selectionSet &&
  !tr.storedMarksSet &&
  Object.keys((tr as unknown as { meta?: object }).meta ?? {}).length === 0

const imeGuardPlugin = () => {
  let current: EditorView | undefined
  return new Plugin({
    view(v) {
      current = v
      return { destroy: () => (current = undefined) }
    },
    filterTransaction: (tr) => !(current?.composing && isNoop(tr)),
  })
}

/** エディタごとに作る（プラグインが自分のエディタの view を覚えるので） */
export const imeGuard = createExtension(() => ({
  key: 'imeGuard',
  prosemirrorPlugins: [imeGuardPlugin()],
}))
