import { createReactInlineContentSpec } from '@blocknote/react'
import PageLinkView from './PageLinkView'

/**
 * 本文中のページへのリンク。保存するのはページ id だけで、
 * 表示時に最新のタイトル・アイコンを引くので、リンク先の改名に追従する。
 */
export const PageLink = createReactInlineContentSpec(
  {
    type: 'pageLink',
    propSchema: { pageId: { default: '' } },
    content: 'none',
  },
  {
    render: ({ inlineContent }) => <PageLinkView pageId={Number(inlineContent.props.pageId)} />,
  },
)
