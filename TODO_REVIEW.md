# 0.4.0〜0.5.3 コードレビューの残課題

v0.5.4で [markdown.rs](backend/src/markdown.rs) の linkCard タイトル、テンプレートの layout/db_schema 引き継ぎ、[sanitize.rs](backend/src/sanitize.rs) のネストオブジェクト素通り、[ogp.rs](backend/src/ogp.rs) の `<meta>` タグ終端検出の5件を修正済み（commit `1b2a44c` / tag `v0.5.4`）。1〜4も対応済み（CLI の export/get への is_template・db_schema・properties 追加、sanitize_properties の fail-closed 化、ogp.rs のクライアント共有化・全体タイムアウト、uploads.rs のサイズ上限）。残りは以下。対応したらこのファイルから削除してよい。

## 未対応（要検討・中程度）

### 5. markdown.rs：Markdown取り込み時、引用(blockquote)内の非段落要素が外に展開される
[backend/src/markdown.rs:170](backend/src/markdown.rs#L170)（`TagEnd::BlockQuote` 処理）

引用ブロックの children を常に `vec![]` で固定し、段落以外の子（コードブロック・リスト等）は引用の外（親の兄弟）に展開してしまう。`brikrs page post` で引用入りのMarkdownを取り込むと構造が崩れる。意図的なコメントあり（設計上の簡略化）なので、直すかどうかは要検討。

### 8. Editor.tsx：useSlashMenuのgetItems・useEditorKeyDown（0.8.1 までは useLinkCardEnter）の戻り値がuseCallbackで固定されていない
[frontend/src/components/Editor.tsx:144](frontend/src/components/Editor.tsx#L144)

キー入力ごとに再生成される関数参照がSuggestionMenuController/onKeyDownCaptureに渡る。内部実装が参照の同一性に依存している場合、キー入力ごとにリスナーの着脱が起きる可能性がある。実害未確認（PLAUSIBLE止まり）。

## 優先度低（簡潔化・重複コード、バグではない）

v0.4.0〜0.5.3レビューの簡潔化/再利用/効率/抽象化観点から出た指摘。実害は小さいが、まとめて直すと良い:

- [backend/src/ogp.rs:246](backend/src/ogp.rs#L246) `decode()` が `charset_param()` と同じロジックを再実装
- [frontend/src/components/DatabaseTable.tsx:251](frontend/src/components/DatabaseTable.tsx#L251) `case 'url'` と `default` が同一JSX
- [frontend/src/index.css](frontend/src/index.css) 1110/1229行目、55-69/1078-1101行目にCSSの重複
- [frontend/src/components/DatabaseColumnsDialog.tsx:34](frontend/src/components/DatabaseColumnsDialog.tsx#L34) `addOption`/`setOption`/`removeOption` の重複ガード
- [frontend/src/components/Formula.tsx](frontend/src/components/Formula.tsx) / [InlineFormula.tsx](frontend/src/components/InlineFormula.tsx) のダイアログ開閉ロジック重複
- 7つのダイアログ（Bookmark/LinkCard/Snippet/DatabaseColumns/Formula/YouTube/Gist）で busy/error の定型処理とmodal-header/footerのJSXが重複
- [backend/src/plaintext.rs:32](backend/src/plaintext.rs#L32) ブロックタイプごとのif分岐が増殖中（0.7.0 で gist も追加）
- [frontend/src/components/PageView.tsx:266](frontend/src/components/PageView.tsx#L266) layoutの表示コンポーネント切り替えがtimeline/databaseだけ特別扱いのネスト三項演算子
- Bookmark/LinkCardが概念的に重複する2本の並行実装（content制約の都合でやむを得ない面あり）

## 0.6系で検討中の機能追加：実装方針メモ

YouTube埋め込み・「/」メニューの「埋め込み」グループ・#7 のダイアログ共通化（`Modal.tsx` の `EditorModal`）・`placeBlock`/`placeBlocks` の統合は 0.6.0、GitHub Gist 埋め込み・`page post` での YouTube 取り込み・#6 site_settings の汎用化は 0.7.0 で実装済み。以下は未着手。着手する際の参考用に、検討済みの実装方針とコスト感を記録しておく。

### Twitter(X)埋め込み（重い・ROI低い、保留推奨）
公式oEmbed（`publish.twitter.com/oembed`）はX社の仕様変更が頻発し不安定。フル機能表示には`widgets.js`（サードパーティscript）の実行許可が必要でCSPを緩めることになる。本文テキストのみ抽出する妥協案でも仕様変更で壊れるリスクが残る。優先度を下げるか、既存のWebリンクカード（Bookmark）機能で代用する運用が無難。

### スラッシュメニューの階層化（現状維持＋小改善で様子見）
調査の結果、[Editor.tsx](frontend/src/components/Editor.tsx)の`useSlashMenu()`は既にBlockNoteの`group`機構でカテゴリ見出し分けされており、フラットリストではない。また入力し始めると絞り込みが働くため、問題になるのは「/」直後の初期表示（絞り込み前）の長さのみ。「埋め込み」グループは 0.6.0 で追加済み。本格的な階層化（カテゴリ選択→サブメニュー）はBlockNoteの`SuggestionMenuController`の前提（フラットな絞り込みリスト）を外れ自前実装コストが増えるため、項目数が実際に問題になってから検討する。

Twitter/Xは保留（Webブックマークで代用）。

## 0.8 で実装：タイムラインの返信ツリー（コメントツリー）

CLI でツッコミ（返信）を投稿するとき、返信先の投稿へリンクを貼るのが面倒で、しかも返信先に飛ぶとただのページとして表示される。返信を「投稿の子ページ」にして、タイムライン上でツリーとして見せる。

方針：新しい列は足さず `parent_id` で木を表す（タイムライン → 投稿 → 返信 → 返信…）。ゴミ箱・下書きは子孫ごと扱う既存の仕組みでそのまま正しく動き、サイドバーはタイムラインの下を展開しないので返信は出ない。

- [x] バックエンド：`timeline_post_ids` をタイムラインの子孫全体に広げる（返信を最近の更新・サイトのフィードから外す）
- [x] バックエンド：`posts` の各投稿に返信の木（新しい順。`page thread` は会話として読めるよう古い順）を付ける
- [x] バックエンド：`GET /pages/{id}/thread`（投稿・返信のどの id からでも、元の投稿以下の木と、所属するタイムラインを返す）
- [x] バックエンド：ピン留めはトップの投稿だけ
- [x] フロント：投稿の下に返信ツリー。字下げは4段で止め、縦線で親子をつなぐ
- [x] フロント：▸/▾ で畳める。閉じているときは「返信N件」。返信が3件を超えたら最初は閉じる。開閉は localStorage に覚える
- [x] フロント：投稿・返信ごとの「返信」ボタン → その場で `QuickPost parentId=…`
- [x] フロント：タイムライン配下のページを `/p/<id>` で開いたらスレッド表示（編集画面はそのままに、上に「← タイムラインへ」と元の投稿からの流れ、下にこのページへの返信の木。返信を開いたときはこのページの位置までスクロール）
- [x] CLI：`page post --reply-to <id>`（`--parent` の別名。タイムラインの配下でなければ警告）
- [x] CLI：`page move <id> --parent <id>`（既存のツッコミ投稿を返信に付け替える）
- [x] CLI：`page get` の front matter にスレッドの位置（`timeline`・`thread_root`）
- [x] フロント：投稿・返信の見出しに「🗑」（下の返信ごとゴミ箱へ。件数は確認の前にサーバーに聞き直す）
- [x] CLI：`page trash <id> --user <username>`
- [x] CLI：`page thread <id>`（スレッド全体を Markdown で。Claude が文脈を読む用）
- 未決：返信をタイムラインのフィードに出すか（当面はトップの投稿だけ）
- 残り（小）：ピン留めの切り替えで投稿が固定欄と一覧の間を移ると、その場で足した返信が再読み込みまで見えなくなる
