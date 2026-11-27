import { createReactBlockSpec } from '@blocknote/react'
import { mapProps } from '../gmaps'
import GoogleMapEmbed from './GoogleMapEmbed'

/**
 * Google マップのブロック。props には地図を表す値（pb・マイマップの mid・場所の名前と座標）だけを持ち、
 * URL は表示のたびに組み立てる。地図の URL の入力は「/」メニューから開くダイアログで行う（GoogleMapDialog）
 */
export const GoogleMap = createReactBlockSpec(
  {
    type: 'googleMap',
    propSchema: {
      mapsPb: { default: '' },
      mapsMid: { default: '' },
      mapsQ: { default: '' },
      mapsLl: { default: '' },
      mapsZoom: { default: '' },
    },
    content: 'none',
  },
  {
    render: ({ block }) => (
      <div className="gmap-block" contentEditable={false}>
        <GoogleMapEmbed map={mapProps(block.props)} />
      </div>
    ),
  },
)
