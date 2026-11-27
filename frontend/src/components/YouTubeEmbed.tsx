import { useState } from 'react'
import { embedUrl, isVideoId, thumbnailUrl, watchUrl } from '../youtube'

/**
 * YouTube 動画（閲覧・編集の両方）。最初はサムネイルと再生ボタンだけを出し、押したときに初めて
 * iframe（youtube-nocookie.com）を読み込む。開いただけで YouTube のプレーヤーやトラッキングを読み込まないように
 */
export default function YouTubeEmbed({ videoId }: { videoId: string }) {
  const [playing, setPlaying] = useState(false)
  if (!isVideoId(videoId)) return <div className="youtube-embed is-invalid muted">動画が設定されていません</div>
  return (
    <div className="youtube-embed">
      {playing ? (
        <iframe
          src={embedUrl(videoId)}
          title="YouTube 動画"
          allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
          allowFullScreen
        />
      ) : (
        <>
          <button type="button" className="youtube-facade" aria-label="動画を再生" onClick={() => setPlaying(true)}>
            <img src={thumbnailUrl(videoId)} alt="" loading="lazy" />
            <span className="youtube-play" aria-hidden />
          </button>
          <a className="youtube-open" href={watchUrl(videoId)} target="_blank" rel="noopener noreferrer">
            YouTube で開く ↗
          </a>
        </>
      )}
    </div>
  )
}
