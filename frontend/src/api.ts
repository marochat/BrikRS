import type { ThemePref } from './theme'
import { shrinkImage } from './imageResize'

export type User = {
  id: number
  username: string
  display_name: string
  is_admin: boolean
  totp_enabled: boolean
  /** カラーテーマ（auto / light / dark） */
  theme: ThemePref
}

export type PageSummary = {
  id: number
  parent_id: number | null
  title: string
  icon: string
  sort_order: number
  /** 下書き（作った人と管理者にだけ見える） */
  is_draft: boolean
  /** 公開範囲（このページ自身の設定。子孫にも効く） */
  visibility: Visibility
  layout: PageLayout
  /** テンプレートとして使えるページ */
  is_template: boolean
  /** ロック（作った人と管理者以外は閲覧だけ。誰も移動・ゴミ箱にできない） */
  is_locked: boolean
}

/** 公開範囲：誰でも / ログインしている人 / 選んだグループの人（と管理者） */
export type Visibility = 'public' | 'members' | 'groups'

/** ユーザーのグループ。user_ids・page_count は管理者にだけ返る */
export type Group = {
  id: number
  name: string
  /** 自分が所属しているか */
  member: boolean
  user_ids?: number[]
  page_count?: number
}

/** データベースページの列の型 */
export type DbColumnKind = 'text' | 'number' | 'select' | 'date' | 'checkbox' | 'url'

export type DbColumn = {
  id: string
  name: string
  kind: DbColumnKind
  /** kind が select のときだけ使う選択肢 */
  options?: string[]
}

/** データベースページの行（子ページ）。本文は含まない */
export type RowSummary = {
  id: number
  parent_id: number | null
  title: string
  icon: string
  sort_order: number
  is_draft: boolean
  properties: Record<string, unknown>
  created_at: string
  updated_at: string
  is_locked: boolean
  /** 自分が編集できるか（ロック中の行は、作った人と管理者だけ） */
  can_edit: boolean
}

export type Snippet = {
  id: number
  name: string
  icon: string
  content: unknown[]
  sort_order: number
}

/** サイト全体の設定（タイトルロゴと、その下の小さな一文、ゲストコメント・リアクションの受け付け） */
export type SiteSettings = {
  title: string
  /** none（無し）/ powered（BrikRS vX.Y。値の名前は 0.4 からの互換のため powered のまま）/ custom（tagline_text） */
  tagline: SiteTagline
  tagline_text: string
  /** タイムラインの投稿へのゲストコメント（承認制）。on でも、ページごとに受け付けたものだけ */
  guest_comments: Switch
  /** タイムラインの投稿・返信へのリアクション。on でも、ページごとに受け付けたものだけ */
  reactions: Switch
  /** ゲストコメントに画像の CAPTCHA を付ける */
  comment_captcha: Switch
  /** アップロードする画像の長い辺の上限（px、'0' は縮小しない）。縮小はブラウザで行う（imageResize.ts） */
  image_max_px: string
}
export type SiteTagline = 'none' | 'powered' | 'custom'
export type Switch = 'off' | 'on'
/** 読み込みに失敗しても画面は出せるよう持っておく既定値（サーバーの SiteSettings::default と同じ） */
export const DEFAULT_SITE: SiteSettings = {
  title: 'BrikRS',
  tagline: 'none',
  tagline_text: '',
  guest_comments: 'off',
  reactions: 'off',
  comment_captcha: 'on',
  image_max_px: '0',
}

/** リアクションの種類（サーバーの interactions::KINDS と同じ並び） */
export const REACTION_KINDS = ['like', 'heart', 'laugh', 'wow', 'party'] as const
export type ReactionKind = (typeof REACTION_KINDS)[number]

/** 投稿・返信ごとのリアクションとコメントの件数 */
export type InteractionStats = {
  reactions: Partial<Record<ReactionKind, number>>
  /** この訪問者が付けた種類 */
  mine: ReactionKind[]
  /** 公開中のコメントの数（タイムラインの直下の投稿だけ） */
  comments: number
}

/** タイムラインでのゲストコメント・リアクションの受け付けと、件数（何も無い投稿は items に入らない） */
export type Interactions = {
  comments: boolean
  reactions: boolean
  captcha: boolean
  items: Record<string, InteractionStats>
  /** ゲストコメントを受け付けない投稿（ログインしていない人に見えない下書きなど）。コメント欄を出さない */
  no_guest_comments?: number[]
}

/** 公開中のコメントと、その下の管理者の返信 */
export type GuestComment = {
  id: number
  name: string
  body: string
  created_at: string
  /** 管理者の返信 */
  is_staff: boolean
  replies: GuestComment[]
}

/** コメントのフォーム用トークン。image は CAPTCHA の画像（data: URL）。CAPTCHA を使わない設定なら null */
export type CommentForm = { token: string; image: string | null; expires_at: string }

/** 受け付けたゲストコメント（承認待ち）。delete_token は自分で消すためのもの */
export type PostedComment = { id: number; delete_token: string; name: string; body: string; created_at: string }

/** 管理画面のコメント一覧の1件 */
export type AdminComment = {
  id: number
  page_id: number
  page_title: string
  page_icon: string
  parent_id: number | null
  name: string
  body: string
  status: 'pending' | 'approved'
  is_staff: boolean
  link_count: number
  ip: string | null
  user_agent: string | null
  created_at: string
  approved_at: string | null
}

/** Web ブックマーク（リンクカード）の内容。image は取り込んだ画像（/uploads/...）か空 */
export type BookmarkData = { url: string; title: string; description: string; site_name: string; image: string }

export type GistFile = { name: string; language: string; content: string; truncated: boolean }
/** Gist の内容（挿入時にサーバーが GitHub の API から取ってくる） */
export type GistData = { id: string; url: string; owner: string; description: string; files: GistFile[] }

/** X の投稿の内容（挿入時にサーバーが X の oEmbed から取ってくる）。handle は @ なし、読み取れなければ空 */
export type TweetData = { id: string; handle: string; author: string; body: string; date: string; media: boolean }

/** page（通常）/ timeline（子ページを投稿として新しい順に並べる）/ database（子ページを型付きの行として表で並べる） */
export type PageLayout = 'page' | 'timeline' | 'database'

export type RecentItem = {
  id: number
  title: string
  icon: string
  is_draft: boolean
  updated_at: string
  updated_by: string | null
  snippet: string
}

export type PageDetail = {
  id: number
  parent_id: number | null
  title: string
  icon: string
  content: unknown[]
  created_by: string | null
  /** 作った人の id（下書き・公開範囲を変えられるのは、作った人と管理者） */
  created_by_id: number | null
  updated_by: string | null
  created_at: string
  updated_at: string
  /** 保存のたびに増える版番号（同時編集の衝突検知用） */
  version: number
  is_draft: boolean
  visibility: Visibility
  /** visibility が groups のときに見られるグループ */
  group_ids: number[]
  layout: PageLayout
  /** タイムラインの先頭に固定 */
  is_pinned: boolean
  is_template: boolean
  /** データベースページ（layout = "database"）の列定義 */
  db_schema: DbColumn[] | null
  /** 行（データベースページの子ページ）の列の値 */
  properties: Record<string, unknown> | null
  /** タイムラインのページで、投稿へのゲストコメントを受け付けるか */
  allow_guest_comments: boolean
  /** タイムラインのページで、投稿・返信へのリアクションを受け付けるか */
  allow_reactions: boolean
  /** ロック（作った人と管理者以外は閲覧だけ。誰も移動・ゴミ箱にできない） */
  is_locked: boolean
}

/** タイムラインの投稿・返信と、その下の返信の木（返信は新しい順） */
export type PostNode = PageDetail & { replies: PostNode[] }

/** タイムラインの投稿・返信のスレッド */
export type ThreadDetail = {
  /** 所属するタイムライン */
  timeline: PageSummary
  /** 元の投稿から親までの投稿（古い順。元の投稿を開いているときは空） */
  ancestors: PageDetail[]
  /** このページへの返信の木 */
  replies: PostNode[]
  /** このページと返信の、ゲストコメント・リアクション */
  interactions: Interactions
}

/** 月別アーカイブの1か月分（month は YYYY-MM） */
export type ArchiveMonth = { month: string; count: number }

export type SearchHit = { id: number; title: string; icon: string; snippet: string }

export type RevisionSummary = {
  id: number
  title: string
  icon: string
  created_by: string | null
  started_at: string
  updated_at: string
}

export type Revision = {
  id: number
  page_id: number
  title: string
  icon: string
  content: unknown[]
  started_at: string
  updated_at: string
}

export type DailyStat = { date: string; pageviews: number; unique_visitors: number }

export type AccessSummary = {
  start_date: string
  end_date: string
  total_pageviews: number
  total_unique_visitors: number
  total_sessions: number
  bot_pageviews: number
  daily: DailyStat[]
  top_pages: { path: string; title: string | null; icon: string | null; pageviews: number; unique_visitors: number }[]
  referrers: { host: string; pageviews: number }[]
}

export type LoginSummary = {
  start_date: string
  end_date: string
  total_attempts: number
  total_failures: number
  total_successes: number
  unique_failure_ips: number
  unique_attempted_usernames: number
  suspicious_threshold: number
  daily_failures: { date: string; failures: number }[]
  suspicious_ips: {
    ip: string
    failures: number
    successes: number
    distinct_usernames: number
    last_attempt: string
    suspicious: boolean
  }[]
  recent_failures: { created_at: string; username: string; ip: string | null; user_agent: string | null; kind: string }[]
}

export type TrashItem = {
  id: number
  title: string
  icon: string
  descendants: number
  deleted_at: string
  deleted_by: string | null
}

export class ApiError extends Error {
  status: number
  /** エラー応答の本文（409 なら latest に最新のページが入る） */
  data: Record<string, unknown>
  constructor(status: number, message: string, data: Record<string, unknown> = {}) {
    super(message)
    this.status = status
    this.data = data
  }
}

// ログイン中だけ持つ CSRF トークン。変更系リクエストのヘッダーに付ける
let csrfToken: string | null = null

async function request<T>(method: string, path: string, body?: unknown, extra: Record<string, string> = {}): Promise<T> {
  const headers: Record<string, string> = { ...extra }
  const isForm = body instanceof FormData
  // FormData は Content-Type（boundary 付き）をブラウザに任せる
  if (body !== undefined && !isForm) headers['Content-Type'] = 'application/json'
  if (method !== 'GET' && csrfToken) headers['X-CSRF-Token'] = csrfToken
  const res = await fetch(`/api${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : isForm ? body : JSON.stringify(body),
    credentials: 'same-origin',
  })
  const data = await res.json().catch(() => ({}))
  if (!res.ok) throw new ApiError(res.status, data.error ?? `HTTP ${res.status}`, data)
  return data as T
}

type Session = { user: User | null; csrf_token?: string; version?: string }
/** パスワードは通ったが、2段階認証のコードが必要 */
type MfaRequired = { mfa_required: true; mfa_token: string }

export type TotpSetup = { secret: string; uri: string; qr_svg: string }

// サーバーのバージョン（ログインしているときだけ分かる）
let serverVersion: string | null = null
export const getServerVersion = () => serverVersion

function applySession(s: Session): User | null {
  csrfToken = s.csrf_token ?? null
  serverVersion = s.version ?? null
  return s.user
}

export const api = {
  me: async () => applySession(await request<Session>('GET', '/me')),
  /** 2段階認証が有効なユーザーなら { mfaToken } が返り、loginTotp で続ける */
  login: async (username: string, password: string): Promise<{ user: User | null } | { mfaToken: string }> => {
    const r = await request<Session | MfaRequired>('POST', '/login', { username, password })
    if ('mfa_required' in r) return { mfaToken: r.mfa_token }
    return { user: applySession(r) }
  },
  loginTotp: async (mfa_token: string, code: string) =>
    applySession(await request<Session>('POST', '/login/totp', { mfa_token, code })),
  totpStatus: () => request<{ enabled: boolean; recovery_codes_left: number }>('GET', '/me/totp'),
  totpSetup: () => request<TotpSetup>('POST', '/me/totp/setup'),
  totpEnable: (secret: string, code: string) =>
    request<{ recovery_codes: string[] }>('POST', '/me/totp/enable', { secret, code }),
  totpDisable: (password: string, code: string) => request('POST', '/me/totp/disable', { password, code }),
  totpRegenerate: (code: string) => request<{ recovery_codes: string[] }>('POST', '/me/totp/recovery-codes', { code }),
  logout: async () => {
    await request('POST', '/logout')
    csrfToken = null
  },
  setTheme: (theme: ThemePref) => request<User>('PUT', '/me/theme', { theme }),
  changePassword: (current_password: string, new_password: string) =>
    request('POST', '/me/password', { current_password, new_password }),

  site: () => request<SiteSettings>('GET', '/site'),
  updateSite: (s: SiteSettings) => request<SiteSettings>('PUT', '/site', s),

  listPages: () => request<PageSummary[]>('GET', '/pages'),
  getPage: (id: number) => request<PageDetail>('GET', `/pages/${id}`),
  /**
   * content を付けると本文付きで作る（タイトルが空なら本文の1行目から付ける）。icon はテンプレートから作るときに引き継ぐ。
   * access はタイムラインの投稿の公開範囲（省略時は public。親の範囲は引き継がれる）
   */
  createPage: (
    parent_id: number | null,
    title = '',
    content?: unknown[],
    is_draft = false,
    icon?: string,
    access?: { visibility: Visibility; group_ids: number[] },
  ) => request<PageDetail>('POST', '/pages', { parent_id, title, content, is_draft, icon, ...access }),
  /** テンプレートページから作る（本文・アイコン・表示形式・列定義を引き継ぎ、タイトルは自動生成しない） */
  createFromTemplate: (parent_id: number | null, tpl: PageDetail) =>
    request<PageDetail>('POST', '/pages', {
      parent_id,
      title: '',
      content: tpl.content,
      icon: tpl.icon,
      layout: tpl.layout,
      db_schema: tpl.db_schema,
      from_template: true,
    }),
  updateSettings: (
    id: number,
    s: {
      is_draft?: boolean
      visibility?: Visibility
      group_ids?: number[]
      layout?: PageLayout
      is_pinned?: boolean
      is_template?: boolean
      db_schema?: DbColumn[]
      properties?: Record<string, unknown>
      allow_guest_comments?: boolean
      allow_reactions?: boolean
      is_locked?: boolean
    },
  ) => request<PageDetail>('PUT', `/pages/${id}/settings`, s),
  /** データベースページの行（子ページ）を sort_order 順に、列の値付きで */
  rows: (id: number) => request<RowSummary[]>('GET', `/pages/${id}/rows`),
  /**
   * タイムラインの投稿（子ページ）を、返信の木付きで新しい順に。before より古いものを返す。最初の読み込みではピン留めを pinned で別に返す。
   * month（YYYY-MM）を付けるとその月の投稿だけを、ピン留めも区別せずに posts で返す
   */
  posts: (id: number, opts: { before?: number; month?: string | null } = {}) => {
    const q = new URLSearchParams()
    if (opts.before) q.set('before', String(opts.before))
    if (opts.month) {
      q.set('month', opts.month)
      q.set('tz', String(-new Date().getTimezoneOffset()))
    }
    return request<{ pinned: PostNode[]; posts: PostNode[]; has_more: boolean; interactions: Interactions }>(
      'GET',
      `/pages/${id}/posts${q.size ? `?${q}` : ''}`,
    )
  },
  /** タイムラインの投稿・返信のスレッド（タイムラインの配下でなければ 404） */
  thread: (id: number) => request<ThreadDetail>('GET', `/pages/${id}/thread`),
  /** タイムラインの月別アーカイブ（投稿のある月と件数、新しい月から） */
  archive: (id: number) =>
    request<ArchiveMonth[]>('GET', `/pages/${id}/archive?tz=${-new Date().getTimezoneOffset()}`),
  recent: () => request<RecentItem[]>('GET', '/recent'),

  /** リアクションを付ける / 外す。返り値の items にはこの投稿の分だけが入る */
  addReaction: (id: number, kind: ReactionKind) => request<Interactions>('PUT', `/pages/${id}/reactions/${kind}`),
  removeReaction: (id: number, kind: ReactionKind) => request<Interactions>('DELETE', `/pages/${id}/reactions/${kind}`),
  /** 投稿の公開中のコメント（受け付けていなければ enabled: false） */
  comments: (id: number) => request<{ enabled: boolean; comments: GuestComment[] }>('GET', `/pages/${id}/comments`),
  commentForm: (pageId: number) => request<CommentForm>('GET', `/comment-token?page_id=${pageId}`),
  /** ゲストのコメントを送る（必ず承認待ちになる）。website はハニーポット */
  postComment: (pageId: number, c: { name: string; body: string; token: string; captcha: string; website: string }) =>
    request<PostedComment>('POST', `/pages/${pageId}/comments`, c),
  /** 手元に残した自分のコメントの今の状態（id → pending / approved。消えたものは入らない） */
  myComments: (items: { id: number; token: string }[]) =>
    request<Record<string, 'pending' | 'approved'>>('POST', '/comments/mine', { items }),
  /** 自分のコメントを消す（投稿時の delete_token で） */
  deleteMyComment: (id: number, token: string) =>
    request('DELETE', `/comments/${id}/mine`, undefined, { 'X-Delete-Token': token }),
  // 以下は管理者だけ
  adminComments: (status: 'pending' | 'approved', before?: number) =>
    request<{ items: AdminComment[]; has_more: boolean }>(
      'GET',
      `/admin/comments?status=${status}${before ? `&before=${before}` : ''}`,
    ),
  pendingCommentCount: () => request<{ pending: number }>('GET', '/admin/comments/count'),
  approveComment: (id: number) => request('POST', `/comments/${id}/approve`),
  deleteComment: (id: number) => request('DELETE', `/comments/${id}`),
  bulkComments: (ids: number[], action: 'approve' | 'delete') =>
    request<{ count: number }>('POST', '/admin/comments/bulk', { ids, action }),
  replyComment: (id: number, body: string) => request<GuestComment>('POST', `/comments/${id}/reply`, { body }),
  /** base_version が今の版と違えば 409（ApiError.data.latest に最新のページ） */
  updatePage: (id: number, patch: { base_version?: number; title?: string; icon?: string; content?: unknown[] }) =>
    request<PageDetail>('PUT', `/pages/${id}`, patch),
  backlinks: (id: number) => request<PageSummary[]>('GET', `/pages/${id}/backlinks`),
  movePage: (id: number, parent_id: number | null, index?: number) =>
    request<PageSummary[]>('PUT', `/pages/${id}/move`, { parent_id, index }),
  /** ゴミ箱へ移す */
  deletePage: (id: number) => request<{ trashed: number }>('DELETE', `/pages/${id}`),

  search: (q: string) => request<SearchHit[]>('GET', `/search?q=${encodeURIComponent(q)}`),

  listRevisions: (pageId: number) => request<RevisionSummary[]>('GET', `/pages/${pageId}/revisions`),
  getRevision: (id: number) => request<Revision>('GET', `/revisions/${id}`),
  restoreRevision: (id: number) => request<PageDetail>('POST', `/revisions/${id}/restore`),

  listTrash: () => request<TrashItem[]>('GET', '/trash'),
  /** 親がゴミ箱にあって最上位に戻したとき、祖先の制限を受けていたなら下書きにする（made_draft） */
  restoreTrash: (id: number) =>
    request<{ pages: PageSummary[]; made_draft: boolean }>('POST', `/trash/${id}/restore`),
  purgeTrash: (id: number) => request<{ deleted: number }>('DELETE', `/trash/${id}`),
  emptyTrash: () => request<{ deleted: number }>('DELETE', '/trash'),

  /** Web ブックマーク：URL の OGP を取得してカードの内容を返す（画像はこのサーバーに取り込む） */
  bookmark: (url: string) => request<BookmarkData>('POST', '/bookmark', { url }),
  gist: (url: string) => request<GistData>('POST', '/gist', { url }),
  tweet: (url: string) => request<TweetData>('POST', '/tweet', { url }),
  uploadImage: async (file: File) => {
    const fd = new FormData()
    fd.append('file', await shrinkImage(file))
    return request<{ url: string }>('POST', '/uploads', fd)
  },

  /** 画面遷移ごとのページビューの記録（ログインしていない閲覧者のみ。失敗しても無視する） */
  pageview: (body: { path: string; visitor_id: string; session_id: string; referrer?: string }) =>
    fetch('/api/pageview', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      credentials: 'same-origin',
      keepalive: true,
    }).catch(() => undefined),
  // tz：日付の区切りに使う、閲覧している人のタイムゾーン（UTC から東へ何分）
  accessSummary: (days: number) =>
    request<AccessSummary>('GET', `/admin/access?days=${days}&tz=${-new Date().getTimezoneOffset()}`),
  loginSummary: (days: number) =>
    request<LoginSummary>('GET', `/admin/logins?days=${days}&tz=${-new Date().getTimezoneOffset()}`),

  /** ブロックスニペット（「/」メニューから挿入する、よく使う複数ブロックの組み合わせ） */
  listSnippets: () => request<Snippet[]>('GET', '/snippets'),
  createSnippet: (s: { name: string; icon: string; content: unknown[] }) =>
    request<Snippet>('POST', '/snippets', s),
  updateSnippet: (id: number, s: { name?: string; icon?: string; content?: unknown[] }) =>
    request<Snippet>('PUT', `/snippets/${id}`, s),
  deleteSnippet: (id: number) => request('DELETE', `/snippets/${id}`),

  listUsers: () => request<User[]>('GET', '/users'),
  createUser: (u: { username: string; display_name: string; password: string; is_admin: boolean }) =>
    request<User>('POST', '/users', u),
  updateUser: (id: number, patch: { display_name?: string; is_admin?: boolean; password?: string; reset_totp?: boolean }) =>
    request<User>('PUT', `/users/${id}`, patch),
  deleteUser: (id: number) => request('DELETE', `/users/${id}`),

  /** グループの一覧（ログインしている人なら誰でも。作成・変更・削除は管理者だけ） */
  listGroups: () => request<Group[]>('GET', '/groups'),
  createGroup: (name: string) => request<{ id: number; name: string }>('POST', '/groups', { name }),
  updateGroup: (id: number, patch: { name?: string; user_ids?: number[] }) => request('PUT', `/groups/${id}`, patch),
  deleteGroup: (id: number) => request('DELETE', `/groups/${id}`),
}
