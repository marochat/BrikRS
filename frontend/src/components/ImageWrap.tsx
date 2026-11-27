import { type BlockSchema, type InlineContentSchema, type StyleSchema } from '@blocknote/core'
import {
  FormattingToolbar,
  FormattingToolbarController,
  getFormattingToolbarItems,
  useBlockNoteEditor,
  useComponentsContext,
  useEditorState,
} from '@blocknote/react'
import type { ImageWrap } from './WrapImage'

/** 画像の回り込みのボタン。ブロックの定義は WrapImage.ts */

const ICONS: Record<'left' | 'right', React.ReactNode> = {
  // 画像の四角と、反対側に文字の線
  left: (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden>
      <rect x="3" y="5" width="9" height="8" rx="1" />
      <rect x="14" y="5" width="7" height="2" rx="1" />
      <rect x="14" y="9" width="7" height="2" rx="1" />
      <rect x="3" y="15" width="18" height="2" rx="1" />
      <rect x="3" y="19" width="13" height="2" rx="1" />
    </svg>
  ),
  right: (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden>
      <rect x="12" y="5" width="9" height="8" rx="1" />
      <rect x="3" y="5" width="7" height="2" rx="1" />
      <rect x="3" y="9" width="7" height="2" rx="1" />
      <rect x="3" y="15" width="18" height="2" rx="1" />
      <rect x="3" y="19" width="13" height="2" rx="1" />
    </svg>
  ),
}

const LABELS = { left: '左に置いて文字を回り込ませる', right: '右に置いて文字を回り込ませる' }

/** 画像を選んでいるときだけ出る。押すと回り込み、もう一度押すと元に戻す */
function WrapButton({ side }: { side: 'left' | 'right' }) {
  const Components = useComponentsContext()!
  const editor = useBlockNoteEditor<BlockSchema, InlineContentSchema, StyleSchema>()
  const state = useEditorState({
    editor,
    selector: ({ editor }) => {
      if (!editor.isEditable) return undefined
      const blocks = editor.getSelection()?.blocks || [editor.getTextCursorPosition().block]
      const block = blocks[0]
      if (blocks.length !== 1 || block.type !== 'image' || !block.props.url) return undefined
      return { block, wrap: (block.props.wrap as ImageWrap | undefined) ?? 'none' }
    },
  })
  if (!state) return null
  const selected = state.wrap === side
  return (
    <Components.FormattingToolbar.Button
      className="bn-button"
      onClick={() => {
        editor.focus()
        const props: Record<string, unknown> = { wrap: selected ? 'none' : side }
        // 回り込ませるとき、幅が本文の 40% より広ければ 40% に縮める（ハンドルで後から変えられる）
        const width = state.block.props.previewWidth
        const max = Math.round((editor.domElement?.firstElementChild?.clientWidth ?? 0) * 0.4)
        if (!selected && typeof width === 'number' && max > 0 && width > max) props.previewWidth = max
        editor.updateBlock(state.block, { props } as never)
      }}
      isSelected={selected}
      label={LABELS[side]}
      mainTooltip={selected ? '回り込みをやめる' : LABELS[side]}
      icon={ICONS[side]}
    />
  )
}

/** 既定の書式ツールバーに回り込みのボタンを足したもの（BlockNoteView に formattingToolbar={false} を付けて中に置く） */
export function BrikFormattingToolbar() {
  return (
    <FormattingToolbarController
      formattingToolbar={() => {
        const items = getFormattingToolbarItems()
        // 画像の配置（左・中央・右）の後ろに並べる
        const at = items.findIndex((i) => i.key === 'textAlignRightButton') + 1
        items.splice(at, 0, <WrapButton side="left" key="wrapLeftButton" />, <WrapButton side="right" key="wrapRightButton" />)
        return <FormattingToolbar>{items}</FormattingToolbar>
      }}
    />
  )
}
