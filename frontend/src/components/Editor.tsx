import { useEffect, useState, type DragEvent, type KeyboardEvent, type MouseEvent } from 'react'
import { TextSelection } from 'prosemirror-state'
import { PUBLIC_ACCESS, useGroups, type Access } from '../access'
import { AccessSelect } from './AccessPicker'
import {
  BlockNoteSchema,
  defaultBlockSpecs,
  defaultInlineContentSpecs,
  filterSuggestionItems,
  type PartialBlock,
} from '@blocknote/core'
import * as locales from '@blocknote/core/locales'
import {
  getDefaultReactSlashMenuItems,
  SuggestionMenuController,
  useCreateBlockNote,
  type DefaultReactSuggestionItem,
} from '@blocknote/react'
import { BlockNoteView } from '@blocknote/mantine'
import '@blocknote/mantine/style.css'
import { api, type BookmarkData, type GistData, type PageDetail, type TweetData } from '../api'
import { gistProps } from '../gist'
import { tweetProps } from '../tweet'
import type { MapProps } from '../gmaps'
import { useImeDebug } from '../imeDebug'
import { imeGuard } from '../imeGuard'
import { mathInput } from '../mathInput'
import { displayTitle, useApp } from '../AppContext'
import { useTheme } from '../theme'
import { Bookmark } from './Bookmark'
import BookmarkDialog from './BookmarkDialog'
import { Formula } from './Formula'
import { Gist } from './Gist'
import GistDialog from './GistDialog'
import { InlineFormula } from './InlineFormula'
import { InlineImage } from './InlineImage'
import { LinkCard } from './LinkCard'
import LinkCardDialog, { type LinkCardForm } from './LinkCardDialog'
import ImageZoom from './ImageZoom'
import { BrikFormattingToolbar } from './ImageWrap'
import { WrapImage } from './WrapImage'
import { BorderlessTable } from './BorderlessTable'
import { BrikSideMenu } from './BrikSideMenu'
import { PageLink } from './PageLink'
import SnippetDialog from './SnippetDialog'
import { GoogleMap } from './GoogleMap'
import GoogleMapDialog from './GoogleMapDialog'
import { Tweet } from './Tweet'
import TweetDialog from './TweetDialog'
import { YouTube } from './YouTube'
import YouTubeDialog from './YouTubeDialog'

const schema = BlockNoteSchema.create({
  blockSpecs: { ...defaultBlockSpecs, image: WrapImage, table: BorderlessTable, bookmark: Bookmark(), linkCard: LinkCard(), formula: Formula(), youtube: YouTube(), gist: Gist(), tweet: Tweet(), googleMap: GoogleMap() },
  inlineContentSpecs: { ...defaultInlineContentSpecs, pageLink: PageLink, inlineFormula: InlineFormula, inlineImage: InlineImage },
})

type EditorBlock = typeof schema.Block
type BrikEditor = typeof schema.BlockNoteEditor

// Markdown らしい行（見出し・リスト・引用・コードフェンス・表）
const MARKDOWN_LINE = /^\s{0,3}(#{1,6}\s|[-*+]\s|\d+[.)]\s|>\s?|```|~~~|\|.*\|\s*$)/

/**
 * 貼り付けた内容を Markdown として解釈するか。
 * - VS Code からのコピー：BlockNote の既定ではコードブロックになるので、言語が markdown なら Markdown として扱う
 * - HTML も付いているコピー：既定では HTML が優先されるので、プレーンテキストが明らかに Markdown ならそちらを使う
 * - プレーンテキストだけのコピー：既定の処理が Markdown として解釈するので任せる
 */
function shouldPasteAsMarkdown(cd: DataTransfer, text: string): boolean {
  const vscode = cd.getData('vscode-editor-data')
  if (vscode) {
    try {
      return JSON.parse(vscode).mode === 'markdown'
    } catch {
      return false
    }
  }
  if (cd.types.includes('blocknote/html') || !cd.types.includes('text/html')) return false
  return text.split(/\r?\n/).filter((l) => MARKDOWN_LINE.test(l)).length >= 2
}

// 投稿欄の案内文（ページ本文の欄と見分けられるように）
const quickPostDictionary = {
  ...locales.ja,
  placeholders: { ...locales.ja.placeholders, emptyDocument: '新しい投稿を書く…（Markdown の貼り付けもできます）' },
}

/** カーソルが表のセルの中にあるか（セルにはブロックを入れられないので、画像は文中の画像で置く） */
const inTable = (editor: BrikEditor) => editor.getTextCursorPosition().block.type === 'table'

/** 画像をアップロードして、カーソルの位置に文中の画像として入れる */
async function insertInlineImages(editor: BrikEditor, files: File[]) {
  for (const file of files.filter((f) => f.type.startsWith('image/'))) {
    try {
      const { url } = await api.uploadImage(file)
      editor.insertInlineContent([{ type: 'inlineImage', props: { url, name: file.name } }])
    } catch (e) {
      alert(`画像をアップロードできませんでした：${(e as Error).message}`)
    }
  }
}

/**
 * 表のセルに画像を落としたら、表の外ではなく落とした位置（セルの中）に入れる（BlockNoteView の onDropCapture）。
 * BlockNote（ProseMirror）より先に拾って止める。セルの外に落としたときは既定の処理（画像ブロック）のまま
 */
function useTableImageDrop(editor: BrikEditor) {
  return (e: DragEvent) => {
    const files = Array.from(e.dataTransfer?.files ?? []).filter((f) => f.type.startsWith('image/'))
    const cell = (e.target as HTMLElement | null)?.closest?.('td, th')
    if (!files.length || !cell || !editor.isEditable || !editor.domElement?.contains(cell)) return
    e.preventDefault()
    e.stopPropagation()
    const view = editor.prosemirrorView
    const at = view?.posAtCoords({ left: e.clientX, top: e.clientY })
    if (view && at) view.dispatch(view.state.tr.setSelection(TextSelection.near(view.state.doc.resolve(at.pos))))
    editor.focus()
    insertInlineImages(editor, files)
  }
}

/**
 * ファイルを選ぶ画面を開いて、選んだ画像を文中の画像として入れる。
 * input はページに置いてから開く（置かないと、ブラウザによっては選んだ後の change が届かない）
 */
function pickInlineImages(editor: BrikEditor) {
  const input = document.createElement('input')
  input.type = 'file'
  input.accept = 'image/png,image/jpeg,image/gif,image/webp'
  input.multiple = true
  input.hidden = true
  const done = () => input.remove()
  input.addEventListener('change', () => {
    const files = Array.from(input.files ?? [])
    done()
    editor.focus()
    insertInlineImages(editor, files)
  })
  input.addEventListener('cancel', done)
  document.body.appendChild(input)
  input.click()
}

export function useBrikEditor(initialContent: unknown[], dictionary = locales.ja) {
  const editor = useCreateBlockNote({
    schema,
    initialContent: initialContent.length ? (initialContent as PartialBlock<typeof schema.blockSchema>[]) : undefined,
    dictionary,
    extensions: [mathInput, imeGuard()],
    uploadFile:async (file) => (await api.uploadImage(file)).url,
    pasteHandler: ({ event, editor, defaultPasteHandler }) => {
      const cd = event.clipboardData
      const text = cd?.getData('text/plain') ?? ''
      // コードブロックの中ではそのまま文字として貼る（既定の処理に任せる）
      const inCode = editor.getTextCursorPosition().block.type === 'codeBlock'
      // 表のセルに画像を貼ったら、表の外ではなくセルの中に入れる
      const images = Array.from(cd?.files ?? []).filter((f) => f.type.startsWith('image/'))
      if (images.length && inTable(editor as BrikEditor)) {
        insertInlineImages(editor as BrikEditor, images)
        return true
      }
      if (cd && text && !inCode && shouldPasteAsMarkdown(cd, text)) {
        editor.pasteMarkdown(text)
        return true
      }
      return defaultPasteHandler()
    },
  })
  useImeDebug(editor)
  return editor
}

/** 「@」でページへのリンクを挿入する候補。allowCreate なら、一致しないときに子ページの作成も出す */
function usePageSuggestions(editor: BrikEditor, parentId: number, allowCreate: boolean) {
  const { pages, reloadPages } = useApp()
  const insertLink = (id: number) =>
    editor.insertInlineContent([{ type: 'pageLink', props: { pageId: String(id) } }, ' '])

  return async (query: string): Promise<DefaultReactSuggestionItem[]> => {
    const items: DefaultReactSuggestionItem[] = pages.map((p) => ({
      title: `${p.icon || '📄'} ${displayTitle(p.title)}`,
      aliases: [p.title],
      group: 'ページへのリンク',
      onItemClick: () => insertLink(p.id),
    }))
    const filtered = filterSuggestionItems(items, query).slice(0, 20)
    const q = query.trim()
    if (allowCreate && q && !pages.some((p) => p.title === q)) {
      filtered.push({
        title: `＋「${q}」を子ページとして作成`,
        group: '新規ページ',
        onItemClick: async () => {
          const p = await api.createPage(parentId, q)
          await reloadPages()
          insertLink(p.id)
        },
      })
    }
    return filtered
  }
}

/** 「/」だけを打った空の段落なら置き換え、そうでなければその後ろに入れる（blockId が無ければ本文の末尾） */
function placeBlocks(editor: BrikEditor, blockId: string | undefined, blocks: PartialBlock<typeof schema.blockSchema>[]) {
  const block = blockId ? editor.getBlock(blockId) : undefined
  if (block && block.type === 'paragraph' && isEmpty([block])) {
    editor.replaceBlocks([block], blocks)
  } else {
    editor.insertBlocks(blocks, block ?? editor.document[editor.document.length - 1], 'after')
  }
}

/**
 * 「/」メニュー：既定の項目に「数式」「スニペット」（画像などと同じグループの末尾）と、
 * 「Web ブックマーク」「リンクカード」「YouTube」「Gist」「X の投稿」「Google マップ」（その後ろの「埋め込み」グループ）を足す。
 * ブックマーク・リンクカード・YouTube・Gist・X の投稿・Google マップはダイアログを開いてから、数式はその場で（空のまま）置いた上でダイアログを開く。
 * スニペットはクリックで即挿入（複数ブロック）、末尾の「スニペットを管理」は管理ダイアログを開く
 */
function useSlashMenu(editor: BrikEditor) {
  const { snippets, reloadSnippets } = useApp()
  const [target, setTarget] = useState<{ kind: 'bookmark' | 'linkCard' | 'youtube' | 'gist' | 'tweet' | 'googleMap' | 'snippets'; blockId: string } | null>(null)

  const getItems = async (query: string) => {
    const items: DefaultReactSuggestionItem[] = getDefaultReactSlashMenuItems(editor)
    const imageItem = (items as (DefaultReactSuggestionItem & { key?: string })[]).find((i) => i.key === 'image')
    const group = imageItem?.group
    // 表のセルの中では、画像を表の外のブロックではなくセルの中に入れる
    if (imageItem && inTable(editor)) {
      imageItem.subtext = '表のセルの中に画像を入れる'
      imageItem.onItemClick = () => pickInlineImages(editor)
    }
    const open = (kind: 'bookmark' | 'linkCard' | 'youtube' | 'gist' | 'tweet' | 'googleMap') => () =>
      setTarget({ kind, blockId: editor.getTextCursorPosition().block.id })
    const embeds: DefaultReactSuggestionItem[] = [
      {
        title: 'Web ブックマーク',
        subtext: 'URL からタイトル・説明・画像のカードを作る',
        aliases: ['bookmark', 'link', 'url', 'ogp', 'ぶっくまーく', 'ブックマーク', 'リンク', 'カード'],
        group: '埋め込み',
        icon: <span aria-hidden>🔖</span>,
        onItemClick: open('bookmark'),
      },
      {
        title: 'リンクカード',
        subtext: '画像と自分で書いた文章のカードに Web リンクを張る',
        aliases: ['linkcard', 'card', 'link', 'りんくかーど', 'リンク', 'カード'],
        group: '埋め込み',
        icon: <span aria-hidden>🪪</span>,
        onItemClick: open('linkCard'),
      },
      {
        title: 'YouTube',
        subtext: 'YouTube の動画を埋め込む（再生するまで読み込まない）',
        aliases: ['youtube', 'video', 'movie', 'embed', 'ゆーちゅーぶ', 'どうが', '動画', '埋め込み'],
        group: '埋め込み',
        icon: <span aria-hidden>▶️</span>,
        onItemClick: open('youtube'),
      },
      {
        title: 'GitHub Gist',
        subtext: 'Gist のコードを埋め込む',
        aliases: ['gist', 'github', 'code', 'embed', 'じすと', 'ギスト', 'コード', '埋め込み'],
        group: '埋め込み',
        icon: <span aria-hidden>🐙</span>,
        onItemClick: open('gist'),
      },
      {
        title: 'X（Twitter）の投稿',
        subtext: 'X の投稿をカードで埋め込む（X の表示は押すまで読み込まない）',
        aliases: ['x', 'twitter', 'tweet', 'post', 'embed', 'ついーと', 'ついったー', 'ツイート', 'ツイッター', 'ぽすと', 'ポスト', '埋め込み'],
        group: '埋め込み',
        icon: <span aria-hidden>𝕏</span>,
        onItemClick: open('tweet'),
      },
      {
        title: 'Google マップ',
        subtext: '地図・マイマップを埋め込む',
        aliases: ['google map', 'map', 'mymap', 'embed', 'ぐーぐるまっぷ', 'まっぷ', 'ちず', '地図', 'マイマップ', '埋め込み'],
        group: '埋め込み',
        icon: <span aria-hidden>🗺️</span>,
        onItemClick: open('googleMap'),
      },
    ]
    const added: DefaultReactSuggestionItem[] = [
      {
        title: '数式（ブロック）',
        subtext: '独立した行に LaTeX の数式を書く',
        aliases: ['formula', 'math', 'latex', 'katex', 'すうしき', '数式'],
        group,
        icon: <span aria-hidden>∑</span>,
        onItemClick: () =>
          placeBlocks(editor, editor.getTextCursorPosition().block.id, [{ type: 'formula', props: { formula: '' } }]),
      },
      {
        title: '数式（インライン）',
        subtext: '文章の中に LaTeX の数式を書く',
        aliases: ['inline formula', 'inline math', 'いんらいん', '数式'],
        group,
        icon: <span aria-hidden>𝑥</span>,
        onItemClick: () => editor.insertInlineContent([{ type: 'inlineFormula', props: { formula: '' } }, ' ']),
      },
      ...snippets.map((s) => ({
        title: `${s.icon || '📎'} ${s.name}`,
        subtext: 'スニペットを挿入',
        aliases: ['snippet', 'すにぺっと', 'スニペット', s.name],
        group,
        icon: <span aria-hidden>{s.icon || '📎'}</span>,
        onItemClick: () =>
          placeBlocks(
            editor,
            editor.getTextCursorPosition().block.id,
            s.content as PartialBlock<typeof schema.blockSchema>[],
          ),
      })),
      {
        title: 'スニペットを管理',
        subtext: '新規作成・編集・削除',
        aliases: ['snippets', 'manage', 'すにぺっと', 'スニペット'],
        group,
        icon: <span aria-hidden>📎</span>,
        onItemClick: () => setTarget({ kind: 'snippets', blockId: editor.getTextCursorPosition().block.id }),
      },
    ]
    // グループは連続している必要があるので、同じグループの最後の項目の後ろに入れる
    const last = items.findLastIndex((i) => i.group === group)
    items.splice(last < 0 ? items.length : last + 1, 0, ...added, ...embeds)
    return filterSuggestionItems(items, query)
  }

  const place = (card: PartialBlock<typeof schema.blockSchema>) => {
    placeBlocks(editor, target?.blockId, [card])
    setTarget(null)
  }

  const insertBookmark = (b: BookmarkData) =>
    place({
      type: 'bookmark',
      props: { url: b.url, title: b.title, description: b.description, siteName: b.site_name, image: b.image },
    })

  const insertYouTube = (videoId: string) => place({ type: 'youtube', props: { videoId } })
  const insertGist = (g: GistData) => place({ type: 'gist', props: gistProps(g) })
  const insertTweet = (t: TweetData) => place({ type: 'tweet', props: tweetProps(t) })
  const insertMap = (map: MapProps) => place({ type: 'googleMap', props: map })

  // 本文は子ブロック。最初から段落を1つ入れておき、すぐ書き始められるようにする
  const insertLinkCard = (f: LinkCardForm) =>
    place({
      type: 'linkCard',
      props: { url: f.url, image: f.image, imagePos: f.imagePos },
      content: f.title,
      children: [{ type: 'paragraph', content: f.description }],
    })

  const close = () => {
    setTarget(null)
    if (target?.kind === 'snippets') reloadSnippets()
  }
  const dialog =
    target?.kind === 'bookmark' ? (
      <BookmarkDialog onDone={insertBookmark} onClose={close} />
    ) : target?.kind === 'linkCard' ? (
      <LinkCardDialog
        mode="create"
        initial={{ url: '', image: '', imagePos: 'left', title: '', description: '' }}
        onDone={insertLinkCard}
        onClose={close}
      />
    ) : target?.kind === 'youtube' ? (
      <YouTubeDialog onDone={insertYouTube} onClose={close} />
    ) : target?.kind === 'gist' ? (
      <GistDialog onDone={insertGist} onClose={close} />
    ) : target?.kind === 'tweet' ? (
      <TweetDialog onDone={insertTweet} onClose={close} />
    ) : target?.kind === 'googleMap' ? (
      <GoogleMapDialog onDone={insertMap} onClose={close} />
    ) : target?.kind === 'snippets' ? (
      <SnippetDialog onClose={close} />
    ) : null
  return { getItems, dialog }
}

/**
 * エディタの keydown を ProseMirror・BlockNote より先に拾う（BlockNoteView の onKeyDownCapture に渡す）。
 * - IME の変換中のキーは、ここで止めて内側へ渡さない。BlockNote はエディタの DOM に直接 keydown を付けていて
 *   （横のメニュー、Backspace でのブロック削除など）変換中かを見ないため、変換中の Backspace で
 *   ひらがなのまま確定してしまうことがある。preventDefault はしないので IME の処理はそのまま。
 *   ProseMirror は変換中のキーをもともと無視するので影響はない
 * - リンクカードのタイトルで Enter を押したら、カードの本文（子ブロック）の先頭へ移る。
 *   BlockNote の既定ではカードの外（後ろ）に新しい段落ができてしまうので
 */
function useEditorKeyDown(editor: BrikEditor) {
  return (e: KeyboardEvent) => {
    if (e.nativeEvent.isComposing) {
      e.stopPropagation()
      return
    }
    if (e.key !== 'Enter' || e.shiftKey || e.ctrlKey || e.metaKey || e.altKey) return
    const block = editor.getTextCursorPosition().block
    if (block.type !== 'linkCard') return
    e.preventDefault()
    e.stopPropagation()
    const first = block.children[0]
    if (first && first.type === 'paragraph' && isEmpty([first])) {
      editor.setTextCursorPosition(first, 'start')
      return
    }
    const [added] = first
      ? editor.insertBlocks([{ type: 'paragraph' }], first, 'before')
      : editor.updateBlock(block, { children: [{ type: 'paragraph' }] }).children
    if (added) editor.setTextCursorPosition(added, 'start')
  }
}

/** 編集中の画像はクリックで選択・サイズ変更になるので、ダブルクリックで拡大表示する */
function useImageZoom() {
  const [zoom, setZoom] = useState<{ src: string; alt: string } | null>(null)
  const onDoubleClick = (e: MouseEvent) => {
    const img = (e.target as HTMLElement).closest('img.bn-visual-media, img.inline-image-img')
    if (!(img instanceof HTMLImageElement)) return
    e.preventDefault()
    setZoom({ src: img.currentSrc || img.src, alt: img.alt })
  }
  const zoomView = zoom && <ImageZoom src={zoom.src} alt={zoom.alt} onClose={() => setZoom(null)} />
  return { onDoubleClick, zoomView }
}

type Props = {
  /** 編集中のページ（「@」から作る新規ページの親になる） */
  pageId: number
  initialContent: unknown[]
  editable: boolean
  onChange?: (blocks: EditorBlock[]) => void
}

/** ページ切り替え時は key で作り直す前提（initialContent は初回のみ使う） */
export default function Editor({ pageId, initialContent, editable, onChange }: Props) {
  const theme = useTheme()
  const editor = useBrikEditor(initialContent)
  const getPageItems = usePageSuggestions(editor, pageId, true)
  const { onDoubleClick, zoomView } = useImageZoom()
  const slash = useSlashMenu(editor)
  const onEditorKeyDown = useEditorKeyDown(editor)
  const onDropCapture = useTableImageDrop(editor)

  return (
    <>
      <BlockNoteView
        editor={editor}
        editable={editable}
        theme={theme}
        slashMenu={false}
        formattingToolbar={false}
        sideMenu={false}
        onChange={() => onChange?.(editor.document)}
        onDoubleClick={onDoubleClick}
        onKeyDownCapture={onEditorKeyDown}
        onDropCapture={onDropCapture}
      >
        {editable && <BrikFormattingToolbar />}
        <BrikSideMenu />
        {editable && <SuggestionMenuController triggerCharacter="/" getItems={slash.getItems} />}
        {editable && <SuggestionMenuController triggerCharacter="@" getItems={getPageItems} />}
        {zoomView}
      </BlockNoteView>
      {slash.dialog}
    </>
  )
}

const isEmpty = (blocks: EditorBlock[]) =>
  blocks.every(
    (b) => b.type === 'paragraph' && (!Array.isArray(b.content) || b.content.length === 0) && !b.children.length,
  )

/** タイムラインの投稿欄。書いて「投稿」すると子ページとして保存する（Ctrl+Enter でも投稿）。「下書き保存」なら下書きの子ページに */
export function QuickPost({
  parentId,
  onPosted,
  onCancel,
  submitLabel = '投稿',
  autoFocus = false,
  chooseAccess = false,
}: {
  parentId: number
  onPosted: (p: PageDetail) => void
  /** 公開範囲を選べるようにする（タイムラインへの投稿。返信は元の投稿の範囲を引き継ぐので出さない） */
  chooseAccess?: boolean
  /** 返信欄のように閉じられるとき */
  onCancel?: () => void
  submitLabel?: string
  autoFocus?: boolean
}) {
  const theme = useTheme()
  const { me, reloadPages } = useApp()
  const groups = useGroups(chooseAccess ? me : null)
  const [access, setAccess] = useState<Access>(PUBLIC_ACCESS)
  const editor = useBrikEditor([], quickPostDictionary)
  // 投稿の中で「@」から新しいページを作ると、それ自体が投稿になってしまうので作成は出さない
  const getPageItems = usePageSuggestions(editor, parentId, false)
  const [empty, setEmpty] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { onDoubleClick, zoomView } = useImageZoom()
  const slash = useSlashMenu(editor)
  const onEditorKeyDown = useEditorKeyDown(editor)
  const onDropCapture = useTableImageDrop(editor)

  useEffect(() => {
    if (autoFocus) editor.focus()
  }, [autoFocus, editor])

  const post = async (draft = false) => {
    if (busy || isEmpty(editor.document)) return
    setBusy(true)
    setError(null)
    try {
      const p = await api.createPage(parentId, '', editor.document, draft, undefined, chooseAccess ? access : undefined)
      editor.replaceBlocks(editor.document, [{ type: 'paragraph' }])
      setEmpty(true)
      await reloadPages()
      onPosted(p)
    } catch (e) {
      setError((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <div
        className="card shadow-sm quick-post"
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
            e.preventDefault()
            post()
          }
        }}
      >
        <div className="quick-post-editor">
          <BlockNoteView
            editor={editor}
            theme={theme}
            slashMenu={false}
            formattingToolbar={false}
            sideMenu={false}
            onChange={() => setEmpty(isEmpty(editor.document))}
            onDoubleClick={onDoubleClick}
            onKeyDownCapture={onEditorKeyDown}
            onDropCapture={onDropCapture}
          >
            <BrikFormattingToolbar />
            <BrikSideMenu />
            <SuggestionMenuController triggerCharacter="/" getItems={slash.getItems} />
            <SuggestionMenuController triggerCharacter="@" getItems={getPageItems} />
            {zoomView}
          </BlockNoteView>
        </div>
        <div className="card-footer d-flex align-items-center gap-2">
          {error && <span className="error me-auto">{error}</span>}
          <span className="muted small ms-auto d-none d-sm-inline">Ctrl+Enter で{submitLabel}</span>
          {onCancel && (
            <button className="btn btn-sm btn-link text-decoration-none" onClick={onCancel} disabled={busy}>
              キャンセル
            </button>
          )}
          {chooseAccess && me && (
            <AccessSelect value={access} onChange={setAccess} groups={groups} me={me} disabled={busy} />
          )}
          <button className="btn btn-sm btn-outline-secondary" onClick={() => post(true)} disabled={busy || empty}>
            下書き保存
          </button>
          <button className="btn btn-sm btn-primary px-3" onClick={() => post()} disabled={busy || empty}>
            {submitLabel}
          </button>
        </div>
      </div>
      {/* 投稿欄の Ctrl+Enter（投稿）がダイアログの中で効かないよう、欄の外に置く */}
      {slash.dialog}
    </>
  )
}
