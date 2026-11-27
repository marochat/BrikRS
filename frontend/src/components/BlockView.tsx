import { useState, type CSSProperties, type MouseEvent, type ReactNode } from 'react'
import { useApp } from '../AppContext'
import { Katex } from '../katex'
import { linkHost } from '../linkHost'
import { navigate } from '../router'
import { isVideoId } from '../youtube'
import { embedUrl, mapProps } from '../gmaps'
import BookmarkCard from './BookmarkCard'
import GistView from './GistView'
import GoogleMapEmbed from './GoogleMapEmbed'
import TweetView from './TweetView'
import ImageZoom from './ImageZoom'
import PageLinkView from './PageLinkView'
import YouTubeEmbed from './YouTubeEmbed'

/*
 * BlockNote のブロック JSON を、BlockNote を読み込まずに表示する閲覧専用の描画。
 * 閲覧するだけの人に BlockNote・Mantine（JS 約1MB）を配らないためのもの。
 * 文字列は React がエスケープし、URL は許可したスキームだけを通す（保存時の検査と二重の鍵）。
 */

type Props = Record<string, unknown>
type Block = { id?: string; type: string; props?: Props; content?: unknown; children?: Block[] }
type Styles = Record<string, unknown>
type Inline =
  | { type: 'text'; text: string; styles?: Styles }
  | { type: 'link'; href: string; content: Inline[] }
  | { type: 'pageLink'; props: { pageId: string } }
  | { type: 'inlineFormula'; props: { formula: string } }
  | { type: 'inlineImage'; props: { url: string; name?: string } }
type TableCell = { type: 'tableCell'; props?: Props; content: Inline[] }
type TableContent = {
  type: 'tableContent'
  headerRows?: number
  headerCols?: number
  columnWidths?: (number | null | undefined)[]
  rows: { cells: (Inline[] | TableCell)[] }[]
}

const COLORS = new Set(['gray', 'brown', 'red', 'orange', 'yellow', 'green', 'blue', 'purple', 'pink'])
const ALIGN = new Set(['left', 'center', 'right', 'justify'])

const str = (v: unknown) => (typeof v === 'string' ? v : '')
const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : undefined)

export function safeUrl(raw: unknown, allowMailto = true): string | undefined {
  const s = str(raw)
  // 空白や制御文字を挟んで "java\tscript:" のようにすり抜けるのを防ぐ
  // eslint-disable-next-line no-control-regex
  const norm = s.replace(/[\s\u0000-\u001f]/g, '').toLowerCase()
  if (!norm) return undefined
  if (/^[/#?]/.test(norm)) return s
  const scheme = norm.match(/^([a-z][a-z0-9+.-]*):/)?.[1]
  if (!scheme) return s
  if (scheme === 'http' || scheme === 'https' || (allowMailto && scheme === 'mailto')) return s
  return undefined
}

function colorClass(text: unknown, bg: unknown): string {
  const c: string[] = []
  if (COLORS.has(str(text))) c.push(`bv-c-${text}`)
  if (COLORS.has(str(bg))) c.push(`bv-bg-${bg}`)
  return c.join(' ')
}

function blockAttrs(b: Block, base: string): { className: string; style?: CSSProperties } {
  const p = b.props ?? {}
  const align = str(p.textAlignment)
  return {
    className: [base, colorClass(p.textColor, p.backgroundColor)].filter(Boolean).join(' '),
    style: ALIGN.has(align) && align !== 'left' ? { textAlign: align as CSSProperties['textAlign'] } : undefined,
  }
}

/** 同じサイト内のリンクは画面遷移だけで済ませる */
function onInternalClick(e: MouseEvent<HTMLAnchorElement>, href: string) {
  if (!href.startsWith('/') || href.startsWith('//') || e.ctrlKey || e.metaKey || e.shiftKey) return
  e.preventDefault()
  navigate(href)
}

/** inLink：リンクの中（ハッシュタグをリンクにすると入れ子になるので、そのままの文字にする） */
function InlineContent({ items, inLink = false }: { items: unknown; inLink?: boolean }) {
  if (!Array.isArray(items)) return null
  return <>{(items as Inline[]).map((it, i) => <InlineItem key={i} item={it} inLink={inLink} />)}</>
}

function InlineItem({ item, inLink }: { item: Inline; inLink: boolean }) {
  switch (item?.type) {
    case 'text':
      return <StyledText text={str(item.text)} styles={item.styles ?? {}} inLink={inLink} />
    case 'link': {
      const href = safeUrl(item.href)
      const inner = <InlineContent items={item.content} inLink />
      if (!href) return <span>{inner}</span>
      const external = /^[a-z]+:/i.test(href) || href.startsWith('//')
      return (
        <a
          className="bv-link"
          href={href}
          onClick={(e) => onInternalClick(e, href)}
          {...(external ? { target: '_blank', rel: 'noopener noreferrer' } : {})}
        >
          {inner}
        </a>
      )
    }
    case 'pageLink':
      return <PageLinkView pageId={Number(item.props?.pageId)} />
    case 'inlineFormula':
      return str(item.props?.formula) ? <Katex formula={str(item.props.formula)} display={false} /> : null
    case 'inlineImage': {
      const url = safeUrl(item.props?.url, false)
      return url ? (
        <span className="inline-image">
          <ZoomImage src={url} alt={str(item.props.name)} />
        </span>
      ) : null
    }
    default:
      return null
  }
}

// ハッシュタグ：「#語」。直前が文字・数字・& # / なら拾わない（URL の #section、&#123; などを除く）
const HASHTAG = /(^|[^\p{L}\p{N}_&#/])#([\p{L}\p{N}_]{1,50})/gu

/** 本文中の #タグ をクリックできるようにする（数字だけのもの、#1 などは除く） */
function withHashtags(text: string): ReactNode {
  const out: ReactNode[] = []
  let last = 0
  for (const m of text.matchAll(HASHTAG)) {
    const tag = m[2]
    if (/^\p{N}+$/u.test(tag)) continue
    const start = m.index + m[1].length
    if (start > last) out.push(text.slice(last, start))
    out.push(<Hashtag key={start} tag={tag} />)
    last = start + 1 + tag.length
  }
  if (last === 0) return text
  out.push(text.slice(last))
  return out
}

function Hashtag({ tag }: { tag: string }) {
  const { openSearch } = useApp()
  return (
    <button type="button" className="hashtag" title={`#${tag} で検索`} onClick={() => openSearch(`#${tag}`)}>
      #{tag}
    </button>
  )
}

function StyledText({ text, styles, inLink }: { text: string; styles: Styles; inLink: boolean }) {
  let node: ReactNode = styles.code || inLink ? text : withHashtags(text)
  if (styles.code) node = <code className="bv-code">{node}</code>
  if (styles.bold) node = <strong>{node}</strong>
  if (styles.italic) node = <em>{node}</em>
  if (styles.underline) node = <u>{node}</u>
  if (styles.strike) node = <s>{node}</s>
  const cls = colorClass(styles.textColor, styles.backgroundColor)
  return cls ? <span className={cls}>{node}</span> : <>{node}</>
}

const plainText = (items: unknown) =>
  Array.isArray(items) ? (items as Inline[]).map((i) => (i?.type === 'text' ? str(i.text) : '')).join('') : ''

export default function BlockView({ blocks }: { blocks: unknown[] }) {
  return (
    <div className="block-view">
      <Blocks blocks={blocks as Block[]} />
    </div>
  )
}

/** 連続するリスト項目は1つの ul / ol にまとめる */
function Blocks({ blocks }: { blocks: Block[] }) {
  const out: ReactNode[] = []
  for (let i = 0; i < blocks.length; ) {
    const t = blocks[i]?.type
    if (t === 'bulletListItem' || t === 'numberedListItem' || t === 'checkListItem') {
      const run: Block[] = []
      while (i < blocks.length && blocks[i].type === t) run.push(blocks[i++])
      const key = run[0].id ?? `l${i}`
      const items = run.map((b, j) => <ListItem key={b.id ?? j} block={b} />)
      if (t === 'numberedListItem')
        out.push(
          <ol key={key} className="bv-ol" start={num(run[0].props?.start)}>
            {items}
          </ol>,
        )
      else
        out.push(
          <ul key={key} className={t === 'checkListItem' ? 'bv-checklist' : 'bv-ul'}>
            {items}
          </ul>,
        )
    } else {
      out.push(<BlockItem key={blocks[i].id ?? i} block={blocks[i]} />)
      i++
    }
  }
  return <>{out}</>
}

function Children({ block }: { block: Block }) {
  if (!block.children?.length) return null
  return (
    <div className="bv-children">
      <Blocks blocks={block.children} />
    </div>
  )
}

function ListItem({ block }: { block: Block }) {
  const a = blockAttrs(block, 'bv-li')
  const checked = block.type === 'checkListItem' && block.props?.checked === true
  return (
    <li className={`${a.className}${checked ? ' checked' : ''}`} style={a.style}>
      {block.type === 'checkListItem' && <input type="checkbox" checked={checked} readOnly tabIndex={-1} />}
      <span className="bv-li-text">
        <InlineContent items={block.content} />
      </span>
      {!!block.children?.length && <Blocks blocks={block.children} />}
    </li>
  )
}

function BlockItem({ block: b }: { block: Block }) {
  const p = b.props ?? {}
  switch (b.type) {
    case 'heading': {
      const level = Math.min(Math.max(num(p.level) ?? 1, 1), 6)
      const H = `h${level}` as 'h1'
      const a = blockAttrs(b, `bv-h bv-h${level}`)
      const heading = (
        <H className={a.className} style={a.style} data-id={b.id}>
          <InlineContent items={b.content} />
        </H>
      )
      if (p.isToggleable)
        return (
          <details className="bv-toggle">
            <summary>{heading}</summary>
            <Children block={b} />
          </details>
        )
      return (
        <>
          {heading}
          <Children block={b} />
        </>
      )
    }
    case 'toggleListItem': {
      const a = blockAttrs(b, 'bv-p')
      return (
        <details className="bv-toggle">
          <summary className={a.className} style={a.style}>
            <InlineContent items={b.content} />
          </summary>
          <Children block={b} />
        </details>
      )
    }
    case 'quote': {
      const a = blockAttrs(b, 'bv-quote')
      return (
        <>
          <blockquote className={a.className} style={a.style}>
            <InlineContent items={b.content} />
          </blockquote>
          <Children block={b} />
        </>
      )
    }
    case 'codeBlock':
      return (
        <pre className="bv-pre" data-language={str(p.language) || undefined}>
          <code>{plainText(b.content)}</code>
        </pre>
      )
    case 'divider':
      return <hr className="bv-hr" />
    case 'pageBreak':
      return <hr className="bv-pagebreak" />
    case 'table':
      return <Table content={b.content as TableContent} borderless={p.borderless === true} />
    case 'image': {
      const url = safeUrl(p.url, false)
      if (!url) return null
      const caption = str(p.caption)
      // 回り込み（WrapImage.ts）。幅と float は CSS で付ける
      const wrap = p.wrap === 'left' || p.wrap === 'right' ? ` bv-wrap bv-wrap-${p.wrap}` : ''
      const a = blockAttrs(b, `bv-figure${wrap}`)
      // 回り込む画像はハンドルで変えた幅（previewWidth）を枠の幅にする。無ければ CSS の 40%
      const width = wrap ? num(p.previewWidth) : undefined
      const style = width ? { ...a.style, width } : a.style
      return (
        <figure className={a.className} style={style}>
          <ZoomImage src={url} alt={caption || str(p.name)} width={num(p.previewWidth)} />
          {caption && <figcaption>{caption}</figcaption>}
        </figure>
      )
    }
    case 'bookmark': {
      const url = safeUrl(p.url, false)
      if (!url) return null
      const image = safeUrl(p.image, false) ?? ''
      return (
        <div className="bookmark-block">
          <BookmarkCard
            b={{ url, title: str(p.title), description: str(p.description), siteName: str(p.siteName), image }}
            link
          />
        </div>
      )
    }
    case 'linkCard': {
      const url = safeUrl(p.url, false)
      const image = safeUrl(p.image, false)
      const pos = image ? (['left', 'right', 'top'].includes(str(p.imagePos)) ? str(p.imagePos) : 'left') : 'none'
      // カードのどこを押しても開く。ただし本文の中のリンク・画像の拡大・トグルなどはそちらを優先する
      const onClick = (e: MouseEvent<HTMLDivElement>) => {
        if (!url || (e.target as HTMLElement).closest('a, button, summary, input, .bv-zoomable')) return
        window.open(url, '_blank', 'noopener,noreferrer')
      }
      return (
        <div className={`link-card pos-${pos}${url ? ' is-link' : ''}`} onClick={onClick}>
          <div className="link-card-head">
            {image && (
              <div className="link-card-image">
                <img src={image} alt="" loading="lazy" />
              </div>
            )}
            <div className="link-card-title">
              <InlineContent items={b.content} />
            </div>
          </div>
          {!!b.children?.length && (
            <div className="link-card-body">
              <Blocks blocks={b.children} />
            </div>
          )}
          {url && (
            <div className="link-card-foot">
              <a className="link-card-host" href={url} target="_blank" rel="noopener noreferrer">
                {linkHost(url)} ↗
              </a>
            </div>
          )}
        </div>
      )
    }
    case 'gist':
      return (
        <div className="gist-block">
          <GistView url={p.url} owner={p.owner} description={p.description} files={p.files} />
        </div>
      )
    case 'tweet':
      return (
        <div className="tweet-block">
          <TweetView
            tweetId={p.tweetId}
            tweetHandle={p.tweetHandle}
            author={p.author}
            body={p.body}
            date={p.date}
            media={p.media}
          />
        </div>
      )
    case 'youtube': {
      const videoId = str(p.videoId)
      if (!isVideoId(videoId)) return null
      return (
        <div className="youtube-block">
          <YouTubeEmbed videoId={videoId} />
        </div>
      )
    }
    case 'googleMap': {
      const map = mapProps(p)
      if (!embedUrl(map)) return null
      return (
        <div className="gmap-block">
          <GoogleMapEmbed map={map} />
        </div>
      )
    }
    case 'formula': {
      const formula = str(p.formula)
      if (!formula) return null
      return (
        <div className="formula-block">
          <Katex formula={formula} display />
        </div>
      )
    }
    case 'video':
    case 'audio': {
      const url = safeUrl(p.url, false)
      if (!url) return null
      const caption = str(p.caption)
      return (
        <figure className="bv-figure">
          {b.type === 'video' ? (
            <video src={url} controls preload="metadata" width={num(p.previewWidth)} />
          ) : (
            <audio src={url} controls preload="metadata" />
          )}
          {caption && <figcaption>{caption}</figcaption>}
        </figure>
      )
    }
    case 'file': {
      const url = safeUrl(p.url, false)
      if (!url) return null
      return (
        <p className="bv-p">
          <a className="bv-file" href={url} target="_blank" rel="noopener noreferrer">
            📎 {str(p.name) || url}
          </a>
          {str(p.caption) && <span className="bv-file-caption">{str(p.caption)}</span>}
        </p>
      )
    }
    default: {
      // paragraph と、知らない種類のブロック（本文があればそれだけ出す）
      const a = blockAttrs(b, 'bv-p')
      return (
        <>
          <p className={a.className} style={a.style}>
            <InlineContent items={b.content} />
          </p>
          <Children block={b} />
        </>
      )
    }
  }
}

function Table({ content, borderless }: { content: TableContent; borderless: boolean }) {
  if (!content || !Array.isArray(content.rows)) return null
  const headerRows = content.headerRows ?? 0
  const headerCols = content.headerCols ?? 0
  // 列の幅はエディタ（prosemirror-tables）と同じ決め方にする：幅を変えた列は px、変えていない列は最小 120px。
  // 全部の列に幅があれば表の幅はその合計、無い列があれば合計を最小の幅にする
  const widths = (Array.isArray(content.columnWidths) ? content.columnWidths : []).map((w) => num(w))
  const total = widths.reduce<number>((t, w) => t + (w ?? 120), 0)
  const fixed = widths.length > 0 && widths.every((w) => w !== undefined)
  const style = widths.length ? (fixed ? { width: total } : { minWidth: total }) : undefined
  return (
    <div className="bv-table-wrap">
      <table
        className={`bv-table${widths.length ? ' bv-table-sized' : ''}${borderless ? ' bv-table-borderless' : ''}`}
        style={style}
      >
        {widths.length > 0 && (
          <colgroup>
            {widths.map((w, i) => (
              <col key={i} style={w ? { width: w } : undefined} />
            ))}
          </colgroup>
        )}
        <tbody>
          {content.rows.map((row, r) => (
            <tr key={r}>
              {(row.cells ?? []).map((cell, c) => {
                const isObj = !Array.isArray(cell) && cell?.type === 'tableCell'
                const props: Props = isObj ? (cell.props ?? {}) : {}
                const items = isObj ? cell.content : cell
                const Cell = r < headerRows || c < headerCols ? 'th' : 'td'
                const align = str(props.textAlignment)
                return (
                  <Cell
                    key={c}
                    className={colorClass(props.textColor, props.backgroundColor) || undefined}
                    colSpan={num(props.colspan)}
                    rowSpan={num(props.rowspan)}
                    style={ALIGN.has(align) && align !== 'left' ? { textAlign: align as 'center' } : undefined}
                  >
                    <InlineContent items={items} />
                  </Cell>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** クリックで拡大表示する画像 */
function ZoomImage({ src, alt, width }: { src: string; alt: string; width?: number }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <img src={src} alt={alt} width={width} loading="lazy" className="bv-zoomable" onClick={() => setOpen(true)} />
      {open && <ImageZoom src={src} alt={alt} onClose={() => setOpen(false)} />}
    </>
  )
}
