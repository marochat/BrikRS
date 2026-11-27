/** 目次：本文の見出しを取り出し、クリックでその位置へ移動する */

export type TocItem = { id: string; level: number; text: string }

/** 見出しが少ないページには目次を出さない */
export const TOC_MIN_HEADINGS = 2

type Node = { id?: unknown; type?: unknown; props?: { level?: unknown }; content?: unknown; children?: unknown }

function inlineText(content: unknown): string {
  if (!Array.isArray(content)) return ''
  return content
    .map((c) => {
      if (c?.type === 'text') return typeof c.text === 'string' ? c.text : ''
      if (c?.type === 'link') return inlineText(c.content)
      return ''
    })
    .join('')
}

export function headingsOf(blocks: unknown): TocItem[] {
  const out: TocItem[] = []
  const walk = (list: unknown) => {
    if (!Array.isArray(list)) return
    for (const b of list as Node[]) {
      if (b?.type === 'heading' && typeof b.id === 'string') {
        const text = inlineText(b.content).trim()
        const level = typeof b.props?.level === 'number' ? b.props.level : 1
        if (text) out.push({ id: b.id, level, text })
      }
      walk(b?.children)
    }
  }
  walk(blocks)
  return out
}

/** 見出しへ移動する（編集画面の BlockNote も閲覧画面の BlockView も、ブロックに data-id を持つ） */
export function scrollToHeading(id: string) {
  const el = document.querySelector<HTMLElement>(`.editor-wrap [data-id="${CSS.escape(id)}"]`)
  el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}
