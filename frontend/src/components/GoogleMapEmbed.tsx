import { embedUrl, openUrl, type MapProps } from '../gmaps'

/**
 * Google マップ（閲覧・編集の両方）。iframe は loading="lazy" で、画面に近づいてから読み込む。
 * 「Google マップで開く」は地図の右下のボタンと重ならないよう、地図の下に置く。
 * sandbox で、地図の中からこのページ自体を移動させることはできないようにする（Google のリンクは別タブで開く）
 */
export default function GoogleMapEmbed({ map }: { map: MapProps }) {
  const src = embedUrl(map)
  if (!src) return <div className="gmap-embed is-invalid muted">地図が設定されていません</div>
  const open = openUrl(map)
  return (
    <div className="gmap-embed">
      <div className="gmap-frame">
        <iframe
          src={src}
          title={map.mapsQ ? `Google マップ：${map.mapsQ}` : 'Google マップ'}
          loading="lazy"
          referrerPolicy="strict-origin-when-cross-origin"
          sandbox="allow-scripts allow-same-origin allow-popups allow-popups-to-escape-sandbox"
          allowFullScreen
        />
      </div>
      {open && (
        <a className="gmap-open" href={open} target="_blank" rel="noopener noreferrer">
          Google マップで開く ↗
        </a>
      )}
    </div>
  )
}
