import { useEffect, useRef, useState, type ReactNode } from 'react'
import { EMBED_ORIGIN, embedUrl, isHandle, isTweetId, statusUrl } from '../tweet'

type Props = {
  tweetId: unknown
  tweetHandle: unknown
  author: unknown
  body: unknown
  date: unknown
  media: unknown
  actions?: ReactNode
}

const str = (v: unknown) => (typeof v === 'string' ? v : '')

/**
 * X の投稿（閲覧・編集の両方）。挿入したときに保存した投稿者・本文・日付をカードで出す。
 * 「X で表示」を押したときに初めて X の iframe（画像・動画を含む公式の表示）を読み込む。
 * 開いただけで X のスクリプトやトラッキングを読み込まないように
 */
export default function TweetView({ tweetId, tweetHandle, author, body, date, media, actions }: Props) {
  const [live, setLive] = useState(false)
  const id = str(tweetId)
  if (!isTweetId(id)) return <div className="tweet-card is-invalid muted">投稿が設定されていません</div>
  const handle = isHandle(str(tweetHandle)) ? str(tweetHandle) : ''
  const href = statusUrl(id, handle)
  const name = str(author)
  const text = str(body)
  return (
    <div className="tweet-card">
      <div className="tweet-head">
        <span className="tweet-logo" aria-hidden>
          𝕏
        </span>
        <span className="tweet-who">
          {name && <span className="tweet-author">{name}</span>}
          {handle && <span className="tweet-handle">@{handle}</span>}
          {!name && !handle && <span className="tweet-author">X の投稿</span>}
        </span>
        {actions}
      </div>
      {live ? (
        <TweetFrame id={id} />
      ) : (
        <>
          {text ? (
            <p className="tweet-body">{text}</p>
          ) : (
            <p className="tweet-body muted small">本文は保存されていません。「X で表示」で読み込めます。</p>
          )}
          {media === true && <div className="tweet-media muted small">🖼 画像・動画あり（「X で表示」で見られます）</div>}
        </>
      )}
      <div className="tweet-foot">
        <a className="tweet-date" href={href} target="_blank" rel="noopener noreferrer">
          {str(date) || 'X で開く'} ↗
        </a>
        <button
          type="button"
          className="tweet-live"
          title={live ? '保存した内容のカードに戻す' : 'X の表示を読み込む（X のスクリプトが読み込まれます）'}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => setLive(!live)}
        >
          {live ? 'カードに戻す' : 'X で表示'}
        </button>
      </div>
    </div>
  )
}

/**
 * 公式の表示の iframe。高さは中から postMessage（twttr.private.resize）で知らせてくるので、
 * 送り元が platform.twitter.com のこの iframe であることを確かめてから合わせる
 */
function TweetFrame({ id }: { id: string }) {
  const ref = useRef<HTMLIFrameElement>(null)
  const [height, setHeight] = useState<number | null>(null)
  const [dark] = useState(() => document.documentElement.dataset.bsTheme === 'dark')

  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (e.origin !== EMBED_ORIGIN || e.source !== ref.current?.contentWindow) return
      let data: unknown = e.data
      if (typeof data === 'string') {
        try {
          data = JSON.parse(data)
        } catch {
          return
        }
      }
      const embed = (data as { 'twttr.embed'?: { method?: unknown; params?: unknown } } | null)?.['twttr.embed']
      if (embed?.method !== 'twttr.private.resize' || !Array.isArray(embed.params)) return
      const h = (embed.params[0] as { height?: unknown } | undefined)?.height
      if (typeof h === 'number' && Number.isFinite(h)) setHeight(Math.min(Math.max(Math.ceil(h), 80), 4000))
    }
    window.addEventListener('message', onMessage)
    return () => window.removeEventListener('message', onMessage)
  }, [])

  return (
    <div className="tweet-frame">
      <iframe
        ref={ref}
        src={embedUrl(id, dark)}
        title="X の投稿"
        style={height ? { height } : undefined}
        referrerPolicy="strict-origin-when-cross-origin"
        sandbox="allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox"
        allowFullScreen
      />
    </div>
  )
}
