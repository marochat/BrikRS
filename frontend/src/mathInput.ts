import { createExtension } from '@blocknote/core'
import { Plugin } from 'prosemirror-state'

/**
 * 閉じの `$` を打った時点で、直前の `$…$` をインライン数式にする。
 * 開きの `$` の直前が `$`・`\`・英数字でなく、中身の両端が空白でないものだけ
 * （「$5 と $10」のような金額や `$$` には反応しない）
 */
const INLINE = /(?:^|[^$\\\w])\$([^\s$](?:[^$]*[^\s$\\])?)$/

/** 一度に遡って調べる文字数（長い段落で毎回全文を見ないように） */
const LOOKBACK = 500

const inlineFormulaPlugin = new Plugin({
  props: {
    handleTextInput(view, from, to, text) {
      if (text !== '$') return false
      const { state } = view
      const $from = state.doc.resolve(from)
      const parent = $from.parent
      // コードブロックとインラインコードの中では文字のまま
      if (parent.type.spec.code || $from.marks().some((m) => m.type.name === 'code')) return false
      const nodeType = state.schema.nodes.inlineFormula
      if (!nodeType) return false
      // ページリンクなどのインライン要素は1文字ぶんの記号に置き換え、位置をずらさない
      const offset = $from.parentOffset
      const before = parent.textBetween(Math.max(0, offset - LOOKBACK), offset, undefined, '￼')
      const m = INLINE.exec(before)
      if (!m || m[1].includes('￼')) return false
      // 「`$x$`」のように、まだ閉じていないインラインコードの途中なら文字のまま
      const open = before.length - m[1].length - 1
      if ((before.slice(0, open).match(/`/g)?.length ?? 0) % 2 === 1) return false
      const start = from - m[1].length - 1
      view.dispatch(state.tr.replaceWith(start, to, nodeType.create({ formula: m[1] })))
      return true
    },
  },
})

/** Markdown 風の数式入力：`$…$` でインライン数式、行頭の `$$` + 空白、または `$$…$$` だけの行で数式ブロック */
export const mathInput = createExtension({
  key: 'mathInput',
  inputRules: [
    // 空の数式ブロックになり、ダイアログが開く
    { find: /^\$\$\s$/, replace: () => ({ type: 'formula', props: { formula: '' } }) },
    { find: /^\$\$([^$]+)\$\$$/, replace: ({ match }) => ({ type: 'formula', props: { formula: match[1].trim() } }) },
  ],
  prosemirrorPlugins: [inlineFormulaPlugin],
})
