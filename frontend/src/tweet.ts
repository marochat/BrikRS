import type { TweetData } from './api'

/*
 * X（旧 Twitter）の投稿の埋め込み用。サーバーの tweet.rs と同じ規則。
 * ブロックには投稿の ID・@名前と、挿入時にサーバーが oEmbed から取った投稿者・本文・日付を保存し、
 * 投稿のページ・iframe の URL はここで組み立てる（入力された URL をそのまま使わない）
 */

export type TweetProps = { tweetId: string; tweetHandle: string; author: string; body: string; date: string; media: boolean }

export const isTweetId = (s: string) => /^\d{1,20}$/.test(s)
export const isHandle = (s: string) => /^[A-Za-z0-9_]{1,15}$/.test(s)

export const tweetProps = (t: TweetData): TweetProps => ({
  tweetId: t.id,
  tweetHandle: t.handle,
  author: t.author,
  body: t.body,
  date: t.date,
  media: t.media,
})

/** X の投稿のページ。@名前が分からなければ /i/status/<ID>（X が正しい名前へ転送する） */
export const statusUrl = (id: string, handle: string) =>
  isHandle(handle) ? `https://x.com/${handle}/status/${id}` : `https://x.com/i/status/${id}`

/** 公式の表示の iframe（CSP の frame-src で許可している）。widgets.js は使わない */
export const embedUrl = (id: string, dark: boolean) =>
  `https://platform.twitter.com/embed/Tweet.html?id=${id}&embedId=brikrs-${id}&dnt=true&lang=ja&theme=${dark ? 'dark' : 'light'}`

export const EMBED_ORIGIN = 'https://platform.twitter.com'
