import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { configureImageResize } from './imageResize'
import { api, DEFAULT_SITE, type PageSummary, type SiteSettings, type Snippet, type User } from './api'
import { AppContext, type AppState } from './AppContext'
import { navigate, pagePath, useRoute } from './router'
import { setThemePref } from './theme'
import type { TocItem } from './toc'
import { getSessionId, getVisitorId } from './visitor'
import NavBar from './components/NavBar'
import Sidebar from './components/Sidebar'
import PageView from './components/PageView'
import LoginPage from './components/LoginPage'
import AdminPage from './components/AdminPage'
import AccountPage from './components/AccountPage'
import SearchDialog from './components/SearchDialog'
import TrashPage from './components/TrashPage'
import RecentPage from './components/RecentPage'

export default function App() {
  const route = useRoute()
  const [me, setMe] = useState<User | null>(null)
  // ログインしていても、既定は閲覧モード（ページの「編集」ボタンで編集モードにする）
  const [editMode, setEditMode] = useState(false)
  const editing = !!me && editMode
  const [pages, setPages] = useState<PageSummary[]>([])
  const [snippets, setSnippets] = useState<Snippet[]>([])
  const [site, setSite] = useState<SiteSettings>(DEFAULT_SITE)
  const [ready, setReady] = useState(false)
  const [sidebarOpen, setSidebarOpen] = useState(false)
  // 検索ダイアログ：null なら閉じている。文字列は開いたときの検索語（ハッシュタグから開くとき）
  const [search, setSearch] = useState<string | null>(null)
  const [toc, setToc] = useState<TocItem[]>([])
  const [pendingComments, setPendingComments] = useState(0)

  const reloadPages = useCallback(async () => setPages(await api.listPages()), [])
  const reloadSnippets = useCallback(async () => setSnippets(await api.listSnippets()), [])
  // PageView の自動保存処理の依存に入るので、参照が変わらないようにしておく
  const patchPage = useCallback(
    (id: number, patch: Partial<PageSummary>) =>
      setPages((ps) => ps.map((p) => (p.id === id ? { ...p, ...patch } : p))),
    [],
  )

  // アップロード前の画像の縮小の大きさ（api.uploadImage が使う）
  useEffect(() => configureImageResize(site.image_max_px), [site.image_max_px])

  useEffect(() => {
    Promise.all([api.me().then(setMe), api.site().then(setSite), reloadPages()])
      .catch((e) => console.error(e))
      .finally(() => setReady(true))
  }, [reloadPages])

  // ログアウトしたら閲覧モードへ戻す（次にログインしたときも閲覧モードから始める）
  useEffect(() => {
    if (!me) setEditMode(false)
  }, [me])

  // スニペットは編集者の「/」メニューでだけ使うので、ログインしているときだけ読み込む
  useEffect(() => {
    if (me) reloadSnippets().catch((e) => console.error(e))
    else setSnippets([])
  }, [me, reloadSnippets])

  // 今のパス（route は描画のたびに作り直されるので、変化はこの文字列で見る）
  const path = location.pathname

  // 承認待ちのゲストコメントの数（ナビバーのバッジ）。管理者だけ、画面遷移のたびに数え直す
  const isAdmin = !!me?.is_admin
  const refreshPendingComments = useCallback(() => {
    if (!isAdmin) return setPendingComments(0)
    api
      .pendingCommentCount()
      .then((r) => setPendingComments(r.pending))
      .catch(() => {})
  }, [isAdmin])
  useEffect(() => {
    refreshPendingComments()
  }, [refreshPendingComments, path])

  // アクセス解析：画面遷移ごとに記録する（編集者自身の閲覧は数えない）。参照元は最初の1回だけ送る
  const firstView = useRef(true)
  useEffect(() => {
    if (!ready || me) return
    api.pageview({
      path,
      visitor_id: getVisitorId(),
      session_id: getSessionId(),
      referrer: firstView.current ? document.referrer || undefined : undefined,
    })
    firstView.current = false
  }, [ready, me, path])

  // ログインしている人は、アカウントに保存したテーマを使う（別の端末で変えた設定にも追従する）
  const savedTheme = me?.theme
  useEffect(() => {
    if (savedTheme) setThemePref(savedTheme)
  }, [savedTheme])

  // Ctrl+K / ⌘K で検索
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setSearch('')
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [])

  // スマホ幅ではページ移動のたびにサイドバーを閉じる
  // route は描画のたびに作り直されるので、パスの文字列で変化を見る（開いた直後に閉じてしまわないように）
  useEffect(() => setSidebarOpen(false), [path])

  const ctx = useMemo<AppState>(
    () => ({
      me,
      setMe,
      editing,
      setEditing: setEditMode,
      site,
      setSite,
      pages,
      setPages,
      reloadPages,
      snippets,
      reloadSnippets,
      openSearch: (q = '') => setSearch(q),
      patchPage,
      toc,
      setToc,
      pendingComments,
      refreshPendingComments,
    }),
    [me, editing, site, pages, reloadPages, snippets, reloadSnippets, patchPage, toc, pendingComments, refreshPendingComments],
  )

  // トップはルートの先頭ページへ
  useEffect(() => {
    if (!ready || route.name !== 'home') return
    const first = pages
      .filter((p) => p.parent_id === null)
      .sort((a, b) => a.sort_order - b.sort_order)[0]
    if (first) navigate(pagePath(first.id), true)
  }, [ready, route, pages])

  if (!ready) return <div className="loading">読み込み中…</div>

  let main
  switch (route.name) {
    case 'page':
      main = <PageView key={route.id} id={route.id} />
      break
    case 'login':
      main = <LoginPage />
      break
    case 'users':
      main = me?.is_admin ? <AdminPage /> : <Message text="権限がありません" />
      break
    case 'account':
      main = me ? <AccountPage /> : <Message text="ログインが必要です" />
      break
    case 'recent':
      main = <RecentPage />
      break
    case 'trash':
      main = me ? <TrashPage /> : <Message text="ログインが必要です" />
      break
    case 'home':
      main = me ? (
        editing ? (
          <Message text="サイドバーの「＋ 新しいページ」から始めましょう" />
        ) : (
          <div className="message">
            まだページがありません
            <div className="mt-3">
              <button className="btn btn-sm btn-primary" onClick={() => setEditMode(true)}>
                ✎ 編集モードにする
              </button>
            </div>
          </div>
        )
      ) : (
        <Message text="まだページがありません" />
      )
      break
    default:
      main = <Message text="ページが見つかりません" />
  }

  return (
    <AppContext.Provider value={ctx}>
      <NavBar onOpenSidebar={() => setSidebarOpen(true)} />
      <div className="app-body">
        <Sidebar open={sidebarOpen} onClose={() => setSidebarOpen(false)} />
        <main className="app-main">{main}</main>
      </div>
      {search !== null && <SearchDialog initialQuery={search} onClose={() => setSearch(null)} />}
    </AppContext.Provider>
  )
}

function Message({ text }: { text: string }) {
  return <div className="message">{text}</div>
}
