import { scrollToHeading, type TocItem } from '../toc'

/** 目次の一覧（サイドバーとページ上部で共用） */
export default function TocList({ items, onPick }: { items: TocItem[]; onPick?: () => void }) {
  const top = Math.min(...items.map((t) => t.level))
  return (
    <ul className="toc-list">
      {items.map((t) => (
        <li key={t.id} style={{ paddingLeft: (t.level - top) * 12 }}>
          <button
            type="button"
            onClick={() => {
              onPick?.()
              scrollToHeading(t.id)
            }}
          >
            {t.text}
          </button>
        </li>
      ))}
    </ul>
  )
}
