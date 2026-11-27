import type { GistData, GistFile } from './api'

/*
 * Gist ブロックの props。ブロックの props は文字列などの単純な値しか持てないので、ファイルの一覧は JSON 文字列で持つ
 */

export type GistProps = { gistId: string; url: string; owner: string; description: string; files: string }

export const gistProps = (g: GistData): GistProps => ({
  gistId: g.id,
  url: g.url,
  owner: g.owner,
  description: g.description,
  files: JSON.stringify(g.files),
})

/** props.files を読む。形が違うものは捨てる */
export function parseGistFiles(raw: unknown): GistFile[] {
  if (typeof raw !== 'string' || !raw) return []
  try {
    const v: unknown = JSON.parse(raw)
    if (!Array.isArray(v)) return []
    return v.flatMap((f) =>
      f && typeof f.name === 'string' && typeof f.content === 'string'
        ? [{ name: f.name, language: typeof f.language === 'string' ? f.language : '', content: f.content, truncated: !!f.truncated }]
        : [],
    )
  } catch {
    return []
  }
}
