import type { ReactNode } from 'react'
import { parseGistFiles } from '../gist'

type Props = { url: unknown; owner: unknown; description: unknown; files: unknown; actions?: ReactNode }

/**
 * Gist（閲覧・編集の両方）。挿入したときに取ってきて保存した内容を、ファイルごとのコードで出す
 * （GitHub の埋め込み用 script は使わない。コードは BlockView のコードブロックと同じく色付けなし）
 */
export default function GistView({ url, owner, description, files, actions }: Props) {
  // リンク先は Gist のページだけ（サーバーが API の html_url から入れた値）
  const href = typeof url === 'string' && url.startsWith('https://gist.github.com/') ? url : undefined
  const list = parseGistFiles(files)
  const desc = typeof description === 'string' ? description : ''
  const who = typeof owner === 'string' && owner ? owner : 'Gist'
  return (
    <div className="gist-card">
      <div className="gist-head">
        <span className="gist-title">
          <span className="gist-owner">{who}</span>
          {desc && <span className="gist-desc">{desc}</span>}
        </span>
        {actions}
        {href && (
          <a className="gist-open" href={href} target="_blank" rel="noopener noreferrer">
            GitHub で見る ↗
          </a>
        )}
      </div>
      {list.length === 0 && <div className="gist-empty muted">ファイルがありません</div>}
      {list.map((f, i) => (
        <div key={i} className="gist-file">
          <div className="gist-file-name">
            {f.name}
            {f.language && <span className="gist-lang">{f.language}</span>}
          </div>
          <pre className="gist-code">
            <code>{f.content}</code>
          </pre>
          {f.truncated && <div className="gist-truncated muted small">長いので途中までです。続きは GitHub で見てください。</div>}
        </div>
      ))}
    </div>
  )
}
