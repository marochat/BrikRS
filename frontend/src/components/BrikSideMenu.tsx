import { useEffect } from 'react'
import { SideMenuExtension } from '@blocknote/core/extensions'
import {
  BlockColorsItem,
  DragHandleMenu,
  RemoveBlockItem,
  SideMenu,
  SideMenuController,
  type SideMenuProps,
  TableColumnHeaderItem,
  TableRowHeaderItem,
  useBlockNoteEditor,
  useComponentsContext,
  useDictionary,
  useExtensionState,
} from '@blocknote/react'
import { syncTableBorders } from './BorderlessTable'

/** 表のときだけ出る「枠線なし」の切り替え（BorderlessTable.ts） */
function TableBorderItem() {
  const Components = useComponentsContext()!
  const editor = useBlockNoteEditor()
  const block = useExtensionState(SideMenuExtension, { editor, selector: (state) => state?.block })
  if (!block || block.type !== 'table') return null
  const borderless = (block.props as { borderless?: boolean }).borderless === true
  return (
    <Components.Generic.Menu.Item
      className="bn-menu-item"
      checked={borderless}
      onClick={() => editor.updateBlock(block, { props: { borderless: !borderless } } as never)}
    >
      枠線なし
    </Components.Generic.Menu.Item>
  )
}

/** 既定のつまみのメニュー（削除・色・見出しの行と列）に「枠線なし」を足したもの */
function BrikDragHandleMenu() {
  const dict = useDictionary()
  return (
    <DragHandleMenu>
      <RemoveBlockItem>{dict.drag_handle.delete_menuitem}</RemoveBlockItem>
      <BlockColorsItem>{dict.drag_handle.colors_menuitem}</BlockColorsItem>
      <TableRowHeaderItem>{dict.drag_handle.header_row_menuitem}</TableRowHeaderItem>
      <TableColumnHeaderItem>{dict.drag_handle.header_column_menuitem}</TableColumnHeaderItem>
      <TableBorderItem />
    </DragHandleMenu>
  )
}

/** つまみの中身。描画のたびに作り直されないよう、モジュールの外側で定義する（ImageWrap.tsx の BrikToolbarItems と同じ） */
function BrikSideMenuItems(props: SideMenuProps) {
  return <SideMenu {...props} dragHandleMenu={BrikDragHandleMenu} />
}

/**
 * ブロックの左のつまみ（BlockNoteView に sideMenu={false} を付けて中に置く）。
 * あわせて、表の「枠線なし」をエディタの見た目に反映する（BorderlessTable.ts の syncTableBorders）
 */
export function BrikSideMenu() {
  const editor = useBlockNoteEditor()
  useEffect(() => {
    const sync = () => syncTableBorders(editor.domElement, editor.document)
    // 表のノードビューは初回の描画が終わってから DOM に入るので、次のフレームでも揃える
    const frame = requestAnimationFrame(sync)
    const off = editor.onChange(sync)
    return () => {
      cancelAnimationFrame(frame)
      off?.()
    }
  }, [editor])
  return <SideMenuController sideMenu={BrikSideMenuItems} />
}
