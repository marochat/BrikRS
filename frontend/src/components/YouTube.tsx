import { createReactBlockSpec } from '@blocknote/react'
import YouTubeEmbed from './YouTubeEmbed'

/**
 * YouTube 動画のブロック。props には動画 ID だけを持つ（URL は表示のたびに組み立てる）。
 * 動画の URL の入力は「/」メニューから開くダイアログで行う（YouTubeDialog）
 */
export const YouTube = createReactBlockSpec(
  {
    type: 'youtube',
    propSchema: { videoId: { default: '' } },
    content: 'none',
  },
  {
    render: ({ block }) => (
      <div className="youtube-block" contentEditable={false}>
        <YouTubeEmbed videoId={block.props.videoId} />
      </div>
    ),
  },
)
