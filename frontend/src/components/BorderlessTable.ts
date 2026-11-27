import { createExtension, defaultBlockSpecs } from '@blocknote/core'
import { Extension } from '@tiptap/core'

/*
 * 表の「枠線なし」（写真を並べるとき用）。BlockNote の表ブロックに `borderless` の属性を足したもの。
 * 切り替えは左のつまみのメニュー（BrikSideMenu の TableBorderItem）。
 *
 * 表ブロックは TipTap のノードをそのまま使っていて、保存できる属性はノード側で決まる（textColor も
 * BlockNote の TextColorExtension が addGlobalAttributes で足している）。そのため config.propSchema に
 * 足すだけでは保存されず、同じやり方で TipTap の属性も足す。
 *
 * また、表のノードビューは更新のたびに作り直されず、textColor 以外の data-* 属性を書き換えない。
 * 見た目の data-borderless は syncTableBorders で揃える（Editor.tsx で本文が変わるたびに呼ぶ）。
 * 閲覧画面の BlockView は props.borderless を見る
 */
const BorderlessAttribute = Extension.create({
  name: 'brikTableBorderless',
  addGlobalAttributes() {
    return [
      {
        types: ['table'],
        attributes: {
          borderless: {
            default: false,
            keepOnSplit: true,
            parseHTML: (el) => el.getAttribute('data-borderless') === 'true',
            renderHTML: (attrs) => (attrs.borderless ? { 'data-borderless': 'true' } : {}),
          },
        },
      },
    ]
  },
})

const table = defaultBlockSpecs.table

export const BorderlessTable = {
  ...table,
  config: {
    ...table.config,
    propSchema: {
      ...table.config.propSchema,
      borderless: { default: false },
    },
  },
  extensions: [...(table.extensions ?? []), createExtension({ key: 'brik-table-borderless', tiptapExtensions: [BorderlessAttribute] })],
}

type AnyBlock = { id: string; type: string; props?: Record<string, unknown>; children?: AnyBlock[] }

/** 本文の表の borderless を、エディタの DOM の data-borderless に写す */
export function syncTableBorders(root: HTMLElement | undefined, blocks: AnyBlock[]) {
  if (!root) return
  const walk = (bs: AnyBlock[]) => {
    for (const b of bs) {
      if (b.type === 'table') {
        const el = root.querySelector(`.bn-block[data-id="${CSS.escape(b.id)}"] > .bn-block-content[data-content-type="table"]`)
        if (el) el.toggleAttribute('data-borderless', b.props?.borderless === true)
      }
      if (b.children?.length) walk(b.children)
    }
  }
  walk(blocks)
}
