//! 初期データ（サンプルページ）。
//!
//! 新しく入れたとき（ユーザーもページも無い DB）の初回起動で自動的に入れる。`SEED_SAMPLES=false` で無効。
//! 既存の DB には `brikrs seed` で追加できる（既存のページの後ろに足す）。
//! `brikrs seed --update --user <username>` は、入っているサンプルを今の版の内容で上書きする。
//! どのページがどのサンプルかは site_settings の `seed_pages` に記録しておく（記録の無い DB では、
//! 初回だけタイトルと親子関係で探す）。自分で作ったページには触れず、消したサンプルは作り直さない。
//! 本文は短い記法から BlockNote のブロック JSON を組み立てる：
//! `**太字**` `~~取り消し~~` `` `コード` `` `[文字](URL)` `{{ページのキー}}`（ページリンク）

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use anyhow::Context;
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, Set, TransactionTrait, sea_query::Expr,
    sea_query::OnConflict,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    api::pages,
    auth,
    entity::{page, page_revision, site_setting, upload, user},
    plaintext,
};

const BANNER_PNG: &[u8] = include_bytes!("../seed/banner.png");

pub fn enabled() -> bool {
    !std::env::var("SEED_SAMPLES").is_ok_and(|v| matches!(v.as_str(), "0" | "false" | "no"))
}

/// 新しく入れた DB（ユーザーもページも無い）なら投入する
pub async fn seed_if_fresh(db: &DatabaseConnection, upload_dir: &Path) -> anyhow::Result<()> {
    if !enabled()
        || user::Entity::find().count(db).await? > 0
        || page::Entity::find().count(db).await? > 0
    {
        return Ok(());
    }
    let n = seed(db, upload_dir).await?;
    tracing::info!("サンプルページを {n} 件作成しました（SEED_SAMPLES=false で無効）");
    Ok(())
}

// ---------- ブロックの組み立て ----------

struct Ctx {
    ids: HashMap<&'static str, i32>,
    banner: String,
}

fn bid() -> String {
    auth::random_token()[..12].to_string()
}

fn text(s: &str, styles: Value) -> Value {
    json!({ "type": "text", "text": s, "styles": styles })
}

/// 短い記法からインライン要素を作る
fn inline(src: &str, ctx: &Ctx) -> Vec<Value> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut rest = src;
    let flush = |buf: &mut String, out: &mut Vec<Value>| {
        if !buf.is_empty() {
            out.push(text(buf, json!({})));
            buf.clear();
        }
    };
    while !rest.is_empty() {
        // 囲み記法：開き・閉じ・スタイル
        let mut matched = false;
        for (open, close, style) in [
            ("**", "**", json!({"bold": true})),
            ("~~", "~~", json!({"strike": true})),
            ("`", "`", json!({"code": true})),
        ] {
            if let Some(r) = rest.strip_prefix(open) {
                if let Some(end) = r.find(close) {
                    flush(&mut buf, &mut out);
                    out.push(text(&r[..end], style));
                    rest = &r[end + close.len()..];
                    matched = true;
                    break;
                }
            }
        }
        if matched {
            continue;
        }
        if let Some(r) = rest.strip_prefix("{{") {
            if let Some(end) = r.find("}}") {
                flush(&mut buf, &mut out);
                let key = &r[..end];
                match ctx.ids.get(key) {
                    Some(id) => out
                        .push(json!({ "type": "pageLink", "props": { "pageId": id.to_string() } })),
                    // リンク先のサンプルが消されている（--update のとき）：タイトルの文字だけを置く
                    None => buf.push_str(&format!(
                        "「{}」",
                        specs()
                            .iter()
                            .find(|s| s.key == key)
                            .map_or(key, |s| s.title)
                    )),
                }
                rest = &r[end + 2..];
                continue;
            }
        }
        if let Some(r) = rest.strip_prefix('[') {
            if let Some((label, after)) = r.split_once("](") {
                if let Some(end) = after.find(')') {
                    flush(&mut buf, &mut out);
                    out.push(json!({
                        "type": "link",
                        "href": &after[..end],
                        "content": [text(label, json!({}))],
                    }));
                    rest = &after[end + 1..];
                    continue;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        buf.push(c);
        rest = &rest[c.len_utf8()..];
    }
    flush(&mut buf, &mut out);
    out
}

fn block(kind: &str, props: Value, content: Value, children: Vec<Value>) -> Value {
    json!({ "id": bid(), "type": kind, "props": props, "content": content, "children": children })
}

fn p(s: &str, c: &Ctx) -> Value {
    block("paragraph", json!({}), json!(inline(s, c)), vec![])
}
fn p_props(s: &str, props: Value, c: &Ctx) -> Value {
    block("paragraph", props, json!(inline(s, c)), vec![])
}
fn h(level: u8, s: &str, c: &Ctx) -> Value {
    block(
        "heading",
        json!({ "level": level }),
        json!(inline(s, c)),
        vec![],
    )
}
fn ul(s: &str, c: &Ctx) -> Value {
    block("bulletListItem", json!({}), json!(inline(s, c)), vec![])
}
fn ul_kids(s: &str, kids: Vec<Value>, c: &Ctx) -> Value {
    block("bulletListItem", json!({}), json!(inline(s, c)), kids)
}
fn ol(s: &str, c: &Ctx) -> Value {
    block("numberedListItem", json!({}), json!(inline(s, c)), vec![])
}
fn check(done: bool, s: &str, c: &Ctx) -> Value {
    block(
        "checkListItem",
        json!({ "checked": done }),
        json!(inline(s, c)),
        vec![],
    )
}
fn quote(s: &str, c: &Ctx) -> Value {
    block("quote", json!({}), json!(inline(s, c)), vec![])
}
fn toggle(s: &str, kids: Vec<Value>, c: &Ctx) -> Value {
    block("toggleListItem", json!({}), json!(inline(s, c)), kids)
}
fn code(lang: &str, src: &str) -> Value {
    block(
        "codeBlock",
        json!({ "language": lang }),
        json!([text(src, json!({}))]),
        vec![],
    )
}
fn divider() -> Value {
    json!({ "id": bid(), "type": "divider", "props": {}, "children": [] })
}
fn image(url: &str, caption: &str, width: u32) -> Value {
    json!({ "id": bid(), "type": "image", "children": [],
        "props": { "url": url, "caption": caption, "name": "banner.png", "previewWidth": width } })
}
fn table(rows: &[&[&str]], c: &Ctx) -> Value {
    let rows: Vec<Value> = rows
        .iter()
        .map(|r| {
            let cells: Vec<Value> = r
                .iter()
                .map(|s| json!({ "type": "tableCell", "props": {}, "content": inline(s, c) }))
                .collect();
            json!({ "cells": cells })
        })
        .collect();
    json!({ "id": bid(), "type": "table", "props": {}, "children": [],
        "content": { "type": "tableContent", "columnWidths": vec![Value::Null; rows.first().map_or(0, |r| r["cells"].as_array().map_or(0, Vec::len))], "headerRows": 1, "rows": rows } })
}

// ---------- ページの定義 ----------

struct Spec {
    key: &'static str,
    parent: Option<&'static str>,
    title: &'static str,
    icon: &'static str,
    layout: &'static str,
    is_draft: bool,
    is_pinned: bool,
    /// 何時間前に書いたことにするか（タイムラインの並びを自然にする）
    hours_ago: i64,
    body: fn(&Ctx) -> Vec<Value>,
}

const fn page(
    key: &'static str,
    parent: Option<&'static str>,
    title: &'static str,
    icon: &'static str,
    body: fn(&Ctx) -> Vec<Value>,
) -> Spec {
    Spec {
        key,
        parent,
        title,
        icon,
        layout: "page",
        is_draft: false,
        is_pinned: false,
        hours_ago: 0,
        body,
    }
}

fn specs() -> Vec<Spec> {
    vec![
        page("welcome", None, "BrikRS へようこそ", "👋", welcome),
        page("guide", Some("welcome"), "使い方ガイド", "📘", guide),
        page("format", Some("welcome"), "書式サンプル", "🎨", format),
        page("spec", Some("welcome"), "仕様と設計", "📐", spec),
        page(
            "admin",
            Some("welcome"),
            "運用ガイド（管理者向け）",
            "🛠️",
            admin,
        ),
        Spec {
            layout: "timeline",
            ..page("timeline", None, "つぶやき", "💬", timeline)
        },
        Spec {
            is_pinned: true,
            hours_ago: 72,
            ..page("post1", Some("timeline"), "", "", post1)
        },
        Spec {
            hours_ago: 30,
            ..page("post2", Some("timeline"), "", "", post2)
        },
        Spec {
            hours_ago: 5,
            ..page("post3", Some("timeline"), "", "", post3)
        },
        Spec {
            hours_ago: 1,
            ..page("post4", Some("timeline"), "", "", post4)
        },
        Spec {
            is_draft: true,
            ..page("draft", None, "下書きのサンプル", "📝", draft)
        },
    ]
}

fn welcome(c: &Ctx) -> Vec<Value> {
    vec![
        p(
            "**BrikRS**（ブリックス）は、Notion のようにブロックを積んで書く、軽量な wiki / ミニブログです。閲覧は誰でも、編集はログインした編集者だけができます。",
            c,
        ),
        image(
            &c.banner,
            "画像はドラッグ＆ドロップや貼り付けでアップロードできます（クリックで拡大）",
            800,
        ),
        p(
            "このページを含むサンプルは、新しく入れたときに自動で作られます。自由に編集・削除してかまいません。",
            c,
        ),
        h(2, "まず試してみること", c),
        check(true, "右上の「ログイン」から管理者でログインする", c),
        check(
            false,
            "ページ右上の「閲覧｜編集」で「編集」に切り替える（ログインしても最初は閲覧モードです）",
            c,
        ),
        check(
            false,
            "本文で `/` を打って、ブロック（見出し・リスト・表・画像・数式・埋め込みなど）を挿入する",
            c,
        ),
        check(
            false,
            "本文で `@` を打って、他のページへのリンクを入れる",
            c,
        ),
        check(
            false,
            "`Ctrl+K`（Mac は `⌘K`）で「タイムライン」や「#BrikRS」を検索する",
            c,
        ),
        check(
            false,
            "左のツリーでページをドラッグして、並べ替えや入れ子にする",
            c,
        ),
        check(
            false,
            "{{timeline}} に短文を投稿し、投稿に「💬 返信する」で返信する",
            c,
        ),
        check(
            false,
            "「管理」の「サイト」で、サイトのタイトルとロゴの下の一文を変える",
            c,
        ),
        check(
            false,
            "「アカウント」でカラーテーマと 2段階認証を設定する",
            c,
        ),
        check(
            false,
            "ログアウトして、閲覧者からの見え方を確かめる（{{draft}} は見えなくなります）",
            c,
        ),
        h(2, "できること", c),
        table(
            &[
                &["分類", "主な機能"],
                &[
                    "書く",
                    "ブロック編集・自動保存、ページリンク、画像、Markdown の貼り付け、数式（KaTeX）",
                ],
                &[
                    "埋め込み",
                    "Web ブックマーク、リンクカード、YouTube、GitHub Gist、X の投稿、Google マップ",
                ],
                &[
                    "まとめる",
                    "ページのツリー、データベース（型付きの列を持つ表）、テンプレート、ブロックスニペット",
                ],
                &[
                    "発信する",
                    "タイムライン（ミニブログ）、返信、月別アーカイブ、ゲストコメント（承認制）、リアクション、Atom フィード、共有カード",
                ],
                &["探す", "検索、ハッシュタグ、最近の更新、バックリンク、目次"],
                &[
                    "守る",
                    "公開範囲（登録ユーザー・グループ）、下書き、編集履歴、ゴミ箱、同時編集の検知、2段階認証",
                ],
                &[
                    "管理する",
                    "ユーザー、サイトの設定、コメントの承認（メール通知）、アクセス解析、ログイン監視",
                ],
            ],
            c,
        ),
        h(2, "ページの案内", c),
        ul("{{guide}} — 操作方法と機能の一覧", c),
        ul("{{format}} — 使えるブロックと文字の装飾", c),
        ul("{{spec}} — 作りと仕様、制限値", c),
        ul("{{admin}} — 最初の設定、バックアップ、コマンド", c),
        ul("{{timeline}} — ミニブログとして使う例", c),
        quote(
            "サンプルが不要になったら、各ページの「…」メニューから「ゴミ箱へ」で消せます（ゴミ箱は30日で自動的に空になります）。",
            c,
        ),
    ]
}

fn guide(c: &Ctx) -> Vec<Value> {
    vec![
        p(
            "BrikRS の操作をひととおりまとめました。左の「目次」（狭い画面ではページ上部）から移動できます。",
            c,
        ),
        h(2, "閲覧モードと編集モード", c),
        ul(
            "ログインしても最初は閲覧モード（ログインしていないときと同じ見た目）。ページ右上の「閲覧｜編集」で切り替えます",
            c,
        ),
        ul(
            "編集モードはページを移っても続き、読み込み直したときとログアウトしたときに閲覧モードへ戻ります",
            c,
        ),
        ul(
            "見られる人を限ったページ・投稿（下書き・登録ユーザー・グループ）は、閲覧モードでも見られる人には見えます（「下書き」「🔒」の表示付き）",
            c,
        ),
        h(2, "ページを作る・並べる", c),
        ul(
            "左のツリーの「＋ 新しいページ」、ページ右上の「＋ 子ページ」で作成",
            c,
        ),
        ul(
            "ツリーの行をドラッグ：上下の端で前後に、真ん中で子ページとして入れ子に",
            c,
        ),
        ul("タイトルの上の「☺ アイコンを追加」で絵文字のアイコン", c),
        ul(
            "書いた内容は 0.8秒後に自動で保存されます（右上に「保存済み」と出ます）",
            c,
        ),
        h(2, "書く", c),
        ul(
            "`/` でブロックを挿入：見出し、リスト、チェックリスト、トグル、引用、コード、表、画像など",
            c,
        ),
        ul(
            "`@` でページへのリンク。リンク先を改名しても、表示は新しいタイトルに追従します。一致するページが無ければ、その場で子ページとして作れます",
            c,
        ),
        ul(
            "画像はドラッグ＆ドロップ、貼り付け、`/画像` から（PNG / JPEG / GIF / WebP、10MB まで）。閲覧中はクリック、編集中はダブルクリックで拡大",
            c,
        ),
        ul(
            "Markdown を貼り付けると書式になります（VS Code からのコピーにも対応）",
            c,
        ),
        ul(
            "数式：文中で `$…$`、行頭で `$$` + 空白と打つと、LaTeX の数式になります（`/数式` からも）",
            c,
        ),
        ul(
            "**ブロックスニペット**：よく使うブロックの組み合わせを「/」メニューの「スニペットを管理」で登録し、1回で挿入",
            c,
        ),
        h(2, "埋め込み", c),
        p("「/」メニューの「埋め込み」から。", c),
        table(
            &[
                &["ブロック", "内容"],
                &[
                    "Web ブックマーク",
                    "URL を入れると、そのページのタイトル・説明・画像のカードを作る",
                ],
                &[
                    "リンクカード",
                    "画像と自分で書いた文章を1つにしたリンクのカード。ページから読み込んだ内容を下書きにもできる",
                ],
                &[
                    "YouTube",
                    "動画の URL を貼る。再生ボタンを押すまでプレーヤーを読み込まない",
                ],
                &[
                    "GitHub Gist",
                    "Gist の URL を貼ると、ファイルの内容をコードとして保存・表示する（「↻」で取り直し）",
                ],
                &[
                    "X の投稿",
                    "投稿の URL を貼ると、投稿者・本文・日付をカードとして保存・表示する（「↻」で取り直し）。画像・動画は「X で表示」で",
                ],
                &[
                    "Google マップ",
                    "場所の URL、「地図を埋め込む」のコード、マイマップの URL を貼る（マイマップは「リンクを知っている全員」に公開しておく）",
                ],
            ],
            c,
        ),
        h(2, "データベースとテンプレート", c),
        ul(
            "「…」メニューの「データベース」で、子ページを型付きの列（テキスト・数値・選択・日付・チェックボックス・URL）を持つ表として並べます。「⚙ 列を編集」で列を変え、「＋ 行を追加」で行（子ページ）を作ります",
            c,
        ),
        ul(
            "「…」メニューの「テンプレートにする」を付けたページは、「＋ 子ページ」「＋ 新しいページ」の「テンプレートから作成…」で選べます",
            c,
        ),
        h(2, "探す", c),
        table(
            &[
                &["機能", "使い方"],
                &[
                    "検索",
                    "`Ctrl+K` / `⌘K`。スペース区切りで AND 検索。タイトルと本文が対象",
                ],
                &[
                    "ハッシュタグ",
                    "本文の #BrikRS のような語をクリックすると、そのタグで検索",
                ],
                &["最近の更新", "ナビバーから。更新の新しい順"],
                &["バックリンク", "ページ下部の「このページへのリンク」"],
                &["目次", "見出しが2つ以上あるページで自動表示"],
            ],
            c,
        ),
        h(2, "公開範囲と共有", c),
        ul(
            "**公開範囲**：「…」メニューの「公開範囲・下書き…」で、公開（誰でも）・登録ユーザー（ログインしている人）・グループ（選んだグループの人と管理者）から選べます。子ページにも効きます",
            c,
        ),
        ul(
            "**下書き**：同じダイアログで切り替え。作った人と管理者にだけ見えます（子ページも含む）。下書きを外すと、選んでおいた公開範囲になります",
            c,
        ),
        ul(
            "公開範囲・下書きを変えられるのは、ページを作った人と管理者だけです。グループは管理画面の「グループ」で作り、ユーザーを入れます（1人が複数のグループに入れます）",
            c,
        ),
        ul(
            "**ロック**：「…」メニューの「🔐 ロック」で、作った人と管理者のほかは閲覧だけにできます。ロック中は作った人・管理者も移動・ゴミ箱へ移すことはできません（ロックを外してから）。ロックはそのページだけに効き、子ページの追加や返信はできます。子ページにロック中のページがあると、親ページもゴミ箱へ移せません",
            c,
        ),
        ul(
            "**フィード**：`/feed.atom`（最近の更新）、`/p/ページ番号/feed.atom`（タイムライン）",
            c,
        ),
        ul(
            "**共有カード**：X などにページの URL を貼ると、タイトル・冒頭・最初の画像がカードで出ます",
            c,
        ),
        h(2, "タイムライン", c),
        ul(
            "「…」メニューの「タイムライン」表示で、子ページを投稿として新しい順に並べます（例：{{timeline}}）",
            c,
        ),
        ul(
            "上の投稿欄から投稿（`Ctrl+Enter` でも投稿）。投稿ボタンの横で公開範囲（公開・登録ユーザー・グループ）を選べます。「下書き保存」なら自分と管理者だけに見える下書きとして保存",
            c,
        ),
        ul(
            "投稿の「💬 返信する」で返信。返信にも返信できます。「▸ 返信 N件」で畳めます",
            c,
        ),
        ul(
            "**ピン留め**：タイムラインの直下の投稿の「📌 固定」で先頭に固定",
            c,
        ),
        ul(
            "「📅 アーカイブ」で月ごとの投稿を表示（URL の `?month=2026-10` で共有できます）",
            c,
        ),
        ul(
            "**ゲストコメント・リアクション**：管理の「サイト」と、タイムラインのページの「…」メニューの両方で有効にすると、ログインしていない人もコメント（管理者の承認制）と 👍 ❤️ 😂 😮 🎉 のリアクションができます",
            c,
        ),
        h(2, "履歴・ゴミ箱・同時編集", c),
        ul(
            "「履歴」から過去の版を表示・復元。同じ人の15分以内の連続した編集は1つの版にまとめます",
            c,
        ),
        ul(
            "「ゴミ箱へ」で子ページごと移動。ゴミ箱から復元・完全削除。30日で自動的に完全削除",
            c,
        ),
        ul(
            "他の人が先に保存していたら知らせます。「自分の変更で上書き」か「最新を読み込む」を選びます",
            c,
        ),
        h(2, "アカウント", c),
        ul(
            "「アカウント」の「表示」でカラーテーマ（OS の設定に従う / ライト / ダーク）",
            c,
        ),
        ul(
            "2段階認証（認証アプリ）。リカバリーコードは安全な場所に保存してください",
            c,
        ),
        h(2, "キーボード", c),
        table(
            &[
                &["キー", "動作"],
                &["`Ctrl+K` / `⌘K`", "検索"],
                &["`/`", "ブロックの挿入メニュー"],
                &["`@`", "ページへのリンク"],
                &["`Ctrl+Enter`", "タイムラインに投稿"],
                &["`Tab` / `Shift+Tab`", "リストの字下げ / 戻す"],
            ],
            c,
        ),
    ]
}

fn format(c: &Ctx) -> Vec<Value> {
    vec![
        h(1, "見出し1", c),
        h(2, "見出し2", c),
        h(3, "見出し3", c),
        p(
            "文字の装飾：**太字**・~~取り消し線~~・`インラインコード`・[外部リンク](https://www.rust-lang.org/)・ページリンク {{guide}}",
            c,
        ),
        json!({ "id": bid(), "type": "paragraph", "props": {}, "children": [], "content": [
            text("色：", json!({})),
            text("赤", json!({"textColor": "red"})), text(" ", json!({})),
            text("橙", json!({"textColor": "orange"})), text(" ", json!({})),
            text("緑", json!({"textColor": "green"})), text(" ", json!({})),
            text("青", json!({"textColor": "blue"})), text(" ", json!({})),
            text("紫", json!({"textColor": "purple"})), text("　背景：", json!({})),
            text("黄", json!({"backgroundColor": "yellow"})), text(" ", json!({})),
            text("ピンク", json!({"backgroundColor": "pink"})), text("　斜体：", json!({})),
            text("イタリック", json!({"italic": true})), text("　下線：", json!({})),
            text("アンダーライン", json!({"underline": true})),
        ]}),
        p_props(
            "このブロックは背景色付き・中央寄せです",
            json!({"backgroundColor": "blue", "textAlignment": "center"}),
            c,
        ),
        p(
            "ハッシュタグ：#BrikRS #書式 のように書くと、閲覧画面でクリックして検索できます。",
            c,
        ),
        h(2, "リスト", c),
        ul_kids(
            "箇条書き",
            vec![ul("入れ子の箇条書き（Tab で字下げ）", c)],
            c,
        ),
        ul("箇条書き（2つめ）", c),
        ol("番号付きリスト", c),
        ol("2番目", c),
        ol("3番目", c),
        check(true, "チェック済み", c),
        check(false, "未チェック", c),
        toggle(
            "トグル（クリックで開く）",
            vec![p("トグルの中身です。長い補足を畳んでおくのに便利です。", c)],
            c,
        ),
        h(2, "その他のブロック", c),
        p(
            "ほかに、Web ブックマーク・リンクカード・YouTube・GitHub Gist・X の投稿・Google マップの埋め込みが「/」メニューの「埋め込み」にあります（{{guide}} を参照）。",
            c,
        ),
        quote("引用ブロック。出典や補足に。", c),
        json!({ "id": bid(), "type": "paragraph", "props": {}, "children": [], "content": [
            text("インライン数式：", json!({})),
            json!({ "type": "inlineFormula", "props": { "formula": "E = mc^2" } }),
            text("（文中で $…$ と打つ）、下は数式ブロック（行頭で $$ + 空白）", json!({})),
        ]}),
        json!({ "id": bid(), "type": "formula", "children": [],
            "props": { "formula": "\\int_0^\\infty e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2}" } }),
        code("rust", "fn main() {\n    println!(\"Hello, BrikRS!\");\n}"),
        table(
            &[&["左", "中", "右"], &["1", "2", "3"], &["a", "b", "c"]],
            c,
        ),
        divider(),
        image(
            &c.banner,
            "画像（クリックで拡大。幅は編集画面でドラッグして変えられます）",
            400,
        ),
    ]
}

fn spec(c: &Ctx) -> Vec<Value> {
    vec![
        p("BrikRS の作りと仕様のまとめです。", c),
        h(2, "構成", c),
        table(
            &[
                &["層", "技術"],
                &[
                    "バックエンド",
                    "Rust / axum / SeaORM（実行ファイル1つ、常駐メモリ十数MB）",
                ],
                &["データベース", "MariaDB"],
                &[
                    "フロントエンド",
                    "React / TypeScript / BlockNote（編集画面）/ Bootstrap",
                ],
                &["コンテナ", "distroless（シェルなし・非 root）"],
                &["数式", "KaTeX（数式を含むページを開いたときだけ読み込む）"],
            ],
            c,
        ),
        h(2, "データの持ち方", c),
        ul(
            "ページの本文は BlockNote のブロック（JSON）として保存。階層は親ページへの参照で表す",
            c,
        ),
        ul(
            "保存時に、検索用のプレーンテキストとページ間のリンク表を作り直す",
            c,
        ),
        ul(
            "タイムラインの投稿は「タイムライン表示のページの子ページ」、返信は投稿の子ページ。投稿用の別のデータは無い",
            c,
        ),
        ul(
            "データベースの行も子ページ。列の定義は親ページ、値は行のページに持つ",
            c,
        ),
        ul(
            "ゲストコメントとリアクションはページとは別の表に持つ（ゲストはページを作らない）",
            c,
        ),
        ul(
            "Web ブックマークの画像、Gist と X の投稿の内容は、挿入したときにサーバーが取得して保存する（表示のたびに外部へ取りに行かない）",
            c,
        ),
        ul(
            "編集履歴は保存後の状態を版として残す（同じ人の15分以内の編集は1つにまとめる）",
            c,
        ),
        h(2, "公開範囲", c),
        ul(
            "閲覧は誰でも、編集はログインした編集者だけ。編集者は管理者が追加する",
            c,
        ),
        ul(
            "ゲストコメントは管理者が承認するまで公開しない（投稿した本人の画面にだけ「承認待ち」と出る）",
            c,
        ),
        ul(
            "見られない人には、そのページ（と、その子孫）はツリー・検索・最近の更新・フィード・共有カードのどこにも出ない（URL を直接開いても「見つかりません」）",
            c,
        ),
        h(2, "セキュリティ", c),
        ul(
            "パスワードは argon2id でハッシュ化。ログイン失敗は IP 単位・ユーザー名単位で回数を制限",
            c,
        ),
        ul(
            "2段階認証（TOTP）。同じコードの使い回しを防ぎ、リカバリーコードは1回だけ使える",
            c,
        ),
        ul(
            "セッションは HttpOnly の Cookie。変更系のリクエストは CSRF トークンを確認",
            c,
        ),
        ul(
            "本文の URL は保存時と表示時の両方で検査し、`javascript:` などは無効にする",
            c,
        ),
        ul("画像は中身のバイト列で形式を判定し、SVG は受け付けない", c),
        ul(
            "Web ブックマークの取得は http / https の 80・443 番だけ。内部のアドレスとこのサーバー自身には接続しない",
            c,
        ),
        ul(
            "ゲストコメントは画像の CAPTCHA（外部サービスなし）・使い捨てのフォーム用トークン・ハニーポット・回数制限で守る。IP と User-Agent は90日で消す",
            c,
        ),
        ul(
            "YouTube は再生ボタンを押すまで youtube-nocookie.com のプレーヤーを読み込まない",
            c,
        ),
        ul(
            "X の投稿は、カードの「X で表示」を押すまで X（platform.twitter.com）の表示を読み込まない",
            c,
        ),
        ul(
            "Google マップは、地図が画面に近づいたときに Google から読み込む（地図のあるページを見ると Google に通信する）",
            c,
        ),
        h(2, "制限値", c),
        table(
            &[
                &["項目", "値"],
                &["画像", "PNG / JPEG / GIF / WebP、1ファイル 10MB まで"],
                &["検索", "スペース区切りで5語まで、結果は50件まで"],
                &["履歴", "ページごとに新しい100件（REVISION_KEEP で変更）"],
                &[
                    "ゴミ箱",
                    "30日で自動的に完全削除（TRASH_RETENTION_DAYS で変更）",
                ],
                &["セッション", "14日"],
                &["タイトル", "255文字まで"],
                &[
                    "ゲストコメント",
                    "名前30文字・本文1000文字まで、承認待ちは全体で200件まで",
                ],
                &["Gist", "合計 256KB・20ファイルまで"],
            ],
            c,
        ),
        h(2, "URL", c),
        table(
            &[
                &["URL", "内容"],
                &["`/p/ページ番号`", "ページ"],
                &["`/recent`", "最近の更新"],
                &[
                    "`/p/ページ番号?month=2026-10`",
                    "タイムラインの月別アーカイブ",
                ],
                &[
                    "`/feed.atom`",
                    "最近の更新の Atom フィード（タイムラインの投稿は除く）",
                ],
                &["`/p/ページ番号/feed.atom`", "タイムラインの Atom フィード"],
            ],
            c,
        ),
    ]
}

fn admin(c: &Ctx) -> Vec<Value> {
    vec![
        p(
            "サーバーを管理する人向けのメモです。閲覧者にも見えるので、パスワードなどは書かないでください（必要なら {{draft}} のように下書きにします）。",
            c,
        ),
        h(2, "最初にやること", c),
        check(
            false,
            "管理者でログインして、「アカウント」で2段階認証を有効にする",
            c,
        ),
        check(false, "「管理」の「ユーザー」で編集者を追加する", c),
        check(
            false,
            "「管理」の「サイト」で、サイトのタイトルとロゴの下の一文、ゲストコメント・リアクションの受け付けを決める",
            c,
        ),
        check(
            false,
            "`PUBLIC_URL` を公開する URL に設定する（フィードと共有カードに使う）",
            c,
        ),
        check(
            false,
            "ゲストコメントを受け付けるなら、承認待ちのメール通知（SMTP の設定）を入れて `brikrs mail-test` で確かめる",
            c,
        ),
        check(false, "DB と画像のバックアップを設定する", c),
        h(2, "管理画面", c),
        table(
            &[
                &["タブ", "内容"],
                &[
                    "ユーザー",
                    "編集者の追加・削除、パスワードの再設定、2段階認証の解除",
                ],
                &[
                    "サイト",
                    "サイトのタイトル、ロゴの下の一文、ゲストコメント・CAPTCHA・リアクションのスイッチ",
                ],
                &[
                    "コメント",
                    "承認待ち・公開中のゲストコメントの承認・削除（承認待ちの件数はナビバーの「管理」に出ます）",
                ],
                &[
                    "アクセス解析",
                    "ページビュー、訪問者、よく見られているページ、参照元",
                ],
                &[
                    "ログイン監視",
                    "ログイン失敗の推移、IP ごとの失敗と「要注意」の印",
                ],
            ],
            c,
        ),
        h(2, "コマンド", c),
        code(
            "bash",
            "# 管理者を作る（パスワードを聞かれる）\ndocker compose exec -it brikrs_app /app/brikrs create-user admin --admin --name 管理者\n\n# 2段階認証を解除する（認証アプリを失くしたとき）\ndocker compose exec brikrs_app /app/brikrs reset-totp admin\n\n# 掃除（ゴミ箱・古い履歴・未使用の画像）を今すぐ実行\ndocker compose exec brikrs_app /app/brikrs maintenance\n\n# サンプルページを追加する\ndocker compose exec brikrs_app /app/brikrs seed\n\n# 承認待ちの通知メールを試しに送る\ndocker compose exec brikrs_app /app/brikrs mail-test\n\n# ページの一覧・読み出し・投稿（自動投稿用。標準入力は Markdown）\ndocker compose exec brikrs_app /app/brikrs page list\ndocker compose exec brikrs_app /app/brikrs page get 1\necho '本文' | docker compose exec -T brikrs_app /app/brikrs page post --user admin --parent <タイムラインのページ番号>",
        ),
        h(2, "主な設定（環境変数）", c),
        table(
            &[
                &["名前", "内容"],
                &["`DATABASE_URL`", "MariaDB の接続先"],
                &[
                    "`PUBLIC_URL`",
                    "公開する URL（例：https://wiki.example.org）",
                ],
                &[
                    "`SITE_NAME`",
                    "サイト名の初期値（管理画面の「サイト」で変えるとそちらが優先）",
                ],
                &["`COOKIE_SECURE`", "HTTPS で公開するときは true"],
                &["`TRUST_PROXY`", "nginx などの後ろに置くときは true"],
                &[
                    "`TRASH_RETENTION_DAYS`",
                    "ゴミ箱の保持日数（既定 30、0 で無期限）",
                ],
                &[
                    "`REVISION_KEEP`",
                    "ページごとの履歴の数（既定 100、0 で無制限）",
                ],
                &[
                    "`LOG_RETENTION_DAYS`",
                    "アクセス・ログインの記録の保持日数（既定 180）",
                ],
                &[
                    "`GUEST_IP_RETENTION_DAYS`",
                    "ゲストコメント・リアクションの IP と User-Agent の保持日数（既定 90）",
                ],
                &[
                    "`SMTP_SERVER` ほか",
                    "承認待ちのメール通知（`SMTP_PORT`・`SMTP_USER`・`SMTP_PASSWD`・`MAIL_FROM`）",
                ],
                &[
                    "`COMMENT_NOTIFY_TO`",
                    "通知の宛先（カンマ区切り。未設定なら送らない）",
                ],
                &[
                    "`OGP_DENY_IPS`",
                    "Web ブックマークで接続しない宛先（未設定なら `PUBLIC_URL` のホスト）",
                ],
                &[
                    "`SEED_SAMPLES`",
                    "初回起動でサンプルページを作るか（既定 true）",
                ],
            ],
            c,
        ),
        h(2, "バックアップ", c),
        ul(
            "DB（MariaDB の brikrs データベース）と、画像の置き場所（コンテナの `/data`）の両方が必要です",
            c,
        ),
        quote(
            "注意：`docker compose down -v` はボリューム（DB と画像）も消します。コンテナだけを止めるときは `-v` を付けないでください。",
            c,
        ),
    ]
}

fn timeline(c: &Ctx) -> Vec<Value> {
    vec![p(
        "タイムライン表示のページです。子ページが投稿として新しい順に並びます。編集モードでは上に投稿欄が出て、投稿には「💬 返信する」で返信できます。",
        c,
    )]
}

fn post1(c: &Ctx) -> Vec<Value> {
    vec![p(
        "ここはミニブログのサンプルです。上の投稿欄から短文を投稿できます（Ctrl+Enter でも投稿）。この投稿はピン留めしてあるので先頭に出ます。 #BrikRS",
        c,
    )]
}

fn post2(c: &Ctx) -> Vec<Value> {
    vec![p(
        "BrikRS は Rust（axum + SeaORM）と React（BlockNote）で書いています。サーバーの常駐メモリは十数MB。 #Rust #BrikRS",
        c,
    )]
}

fn post3(c: &Ctx) -> Vec<Value> {
    vec![
        p(
            "画像付きの投稿です。投稿の最初の画像は、X などで共有したときのカードにも使われます。",
            c,
        ),
        image(&c.banner, "", 600),
    ]
}

fn post4(c: &Ctx) -> Vec<Value> {
    vec![
        p("Markdown を貼り付けると書式になります。たとえば：", c),
        ul("`- ` で始まる行は箇条書き", c),
        ul("`**太字**` は **太字**", c),
        p(
            "投稿もふつうのページなので、検索・履歴・ゴミ箱がそのまま使えます。 #BrikRS",
            c,
        ),
        p(
            "ゲストコメントとリアクションを受け付けるには、管理の「サイト」と、このページの「…」メニューの両方で有効にします。",
            c,
        ),
    ]
}

fn draft(c: &Ctx) -> Vec<Value> {
    vec![
        p(
            "このページは**下書き**です。作った人と管理者にだけ見え、他の人やログアウトした人にはツリーにも検索にも出ません（サンプルのページは作った人がいないので、管理者だけに見えます）。",
            c,
        ),
        p(
            "公開するには、「…」メニューの「公開範囲・下書き…」で「下書き」を外します。",
            c,
        ),
    ]
}

// ---------- 記録（どのページがどのサンプルか） ----------

/// site_settings に置く名前（サイトの設定の API は自分のフィールド名しか読み書きしないので、表に同居できる）
const RECORD_NAME: &str = "seed_pages";

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
struct Record {
    /// サンプルのキー → ページ番号。null は「消されていた（作り直さない）」
    pages: BTreeMap<String, Option<i32>>,
    /// サンプルの画像のファイル名（上書きのたびに画像を増やさないため）
    banner: Option<String>,
}

async fn load_record(db: &impl ConnectionTrait) -> anyhow::Result<Option<Record>> {
    let Some(row) = site_setting::Entity::find_by_id(RECORD_NAME)
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(
        serde_json::from_str(&row.value).context("seed_pages の記録を読めません")?,
    ))
}

async fn save_record(db: &impl ConnectionTrait, r: &Record) -> anyhow::Result<()> {
    site_setting::Entity::insert(site_setting::ActiveModel {
        name: Set(RECORD_NAME.into()),
        value: Set(serde_json::to_string(r)?),
    })
    .on_conflict(
        OnConflict::column(site_setting::Column::Name)
            .update_column(site_setting::Column::Value)
            .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

// ---------- 投入 ----------

/// サンプルの画像をアップロード済みの扱いで置く（どのページ・履歴からも使われなくなれば定期的な掃除で消える）
async fn put_banner(db: &impl ConnectionTrait, upload_dir: &Path) -> anyhow::Result<String> {
    let filename = format!("{}.png", &auth::random_token()[..32]);
    tokio::fs::create_dir_all(upload_dir).await?;
    tokio::fs::write(upload_dir.join(&filename), BANNER_PNG).await?;
    upload::ActiveModel {
        filename: Set(filename.clone()),
        original_name: Set("banner.png".into()),
        mime: Set("image/png".into()),
        size: Set(BANNER_PNG.len() as i64),
        uploaded_by: Set(None),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(filename)
}

/// 本文が空のページを作る（ページリンクに使う id を先に決めるため）
async fn insert_empty(
    db: &impl ConnectionTrait,
    s: &Spec,
    parent: Option<i32>,
    sort_order: i32,
) -> anyhow::Result<i32> {
    let at = Utc::now() - Duration::hours(s.hours_ago);
    let p = page::ActiveModel {
        parent_id: Set(parent),
        title: Set(s.title.into()),
        icon: Set(s.icon.into()),
        content: Set("[]".into()),
        search_text: Set(Some(String::new())),
        sort_order: Set(sort_order),
        created_by: Set(None),
        updated_by: Set(None),
        created_at: Set(at),
        updated_at: Set(at),
        version: Set(1),
        is_draft: Set(s.is_draft),
        layout: Set(s.layout.into()),
        is_pinned: Set(s.is_pinned),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(p.id)
}

/// 親の下（最上位なら最上位）の末尾の並び順
async fn next_order(db: &impl ConnectionTrait, parent: Option<i32>) -> anyhow::Result<i32> {
    let q = page::Entity::find().order_by_desc(page::Column::SortOrder);
    let q = match parent {
        Some(pid) => q.filter(page::Column::ParentId.eq(pid)),
        None => q.filter(page::Column::ParentId.is_null()),
    };
    Ok(q.one(db).await?.map_or(0, |p| p.sort_order + 1))
}

/// 本文とタイトル（タイトルが空の投稿は本文の1行目から）を組み立てる
fn build(s: &Spec, ctx: &Ctx) -> (Value, String, String) {
    let content = json!((s.body)(ctx));
    let plain = plaintext::extract(&content);
    let title = if s.title.is_empty() {
        // API で投稿したときと同じく、40文字を超えたら「…」で切る
        let line = plain.lines().next().unwrap_or("");
        let mut first: String = line.chars().take(40).collect();
        if line.chars().count() > 40 {
            first.push('…');
        }
        first
    } else {
        s.title.to_string()
    };
    (content, plain, title)
}

/// 作ったばかりのページに本文を入れる
async fn fill(db: &impl ConnectionTrait, id: i32, s: &Spec, ctx: &Ctx) -> anyhow::Result<()> {
    let (content, plain, title) = build(s, ctx);
    page::Entity::update_many()
        .col_expr(page::Column::Content, Expr::value(content.to_string()))
        .col_expr(page::Column::SearchText, Expr::value(plain))
        .col_expr(page::Column::Title, Expr::value(title))
        .filter(page::Column::Id.eq(id))
        .exec(db)
        .await?;
    pages::replace_links(db, id, &content)
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(())
}

/// サンプルページを作る。既存のページがあれば、その後ろ（最上位の末尾）に足す
pub async fn seed(db: &DatabaseConnection, upload_dir: &Path) -> anyhow::Result<usize> {
    let txn = db.begin().await?;
    let banner = put_banner(&txn, upload_dir).await?;
    let top_order = next_order(&txn, None).await?;

    // 1. 本文なしでページを作って id を決める（ページリンクに使うため）
    let specs = specs();
    let mut ctx = Ctx {
        ids: HashMap::new(),
        banner: format!("/uploads/{banner}"),
    };
    let mut order: HashMap<Option<i32>, i32> = HashMap::new();
    for s in &specs {
        let parent = s.parent.map(|k| ctx.ids[k]);
        let next = order
            .entry(parent)
            .or_insert(if parent.is_none() { top_order } else { 0 });
        let id = insert_empty(&txn, s, parent, *next).await?;
        *next += 1;
        ctx.ids.insert(s.key, id);
    }

    // 2. 本文を入れる
    for s in &specs {
        fill(&txn, ctx.ids[s.key], s, &ctx).await?;
    }

    // 3. 次の --update のために、どのページがどのサンプルかを記録する（2回目の seed なら新しい方を記録）
    let record = Record {
        pages: specs
            .iter()
            .map(|s| (s.key.to_string(), Some(ctx.ids[s.key])))
            .collect(),
        banner: Some(banner),
    };
    save_record(&txn, &record).await?;
    txn.commit().await?;
    Ok(specs.len())
}

// ---------- 上書き（--update） ----------

/// 1つのサンプルをどうするか
#[derive(Debug, Clone, PartialEq)]
enum Plan {
    /// 今あるページを上書きする
    Update(i32),
    /// 新しく作る（今の版で足されたサンプル）
    Create,
    /// 触れない（理由）
    Skip(String),
}

/// 計画を立てるときに見るページの情報
#[derive(Debug, Clone)]
struct Existing {
    id: i32,
    parent_id: Option<i32>,
    title: String,
    deleted: bool,
}

/// それぞれのサンプルを、今の DB のどのページに当てるか決める（DB には書き込まない）
fn make_plan(specs: &[Spec], record: Option<&Record>, pages: &[Existing]) -> Vec<Plan> {
    // 記録の無い DB でタイトルを比べるための本文（ページリンクの先は使わない）
    let dummy = Ctx {
        ids: specs.iter().map(|s| (s.key, 1)).collect(),
        banner: String::new(),
    };
    let mut resolved: HashMap<&str, Option<i32>> = HashMap::new();
    let mut plans = Vec::new();
    for s in specs {
        let parent = s.parent.map(|k| resolved.get(k).copied().flatten());
        let plan = match record {
            Some(r) => match r.pages.get(s.key) {
                Some(Some(id)) => match pages.iter().find(|p| p.id == *id) {
                    Some(p) if !p.deleted => Plan::Update(p.id),
                    Some(_) => Plan::Skip(format!("ページ {id} はゴミ箱にあるので触れません")),
                    None => Plan::Skip(format!("ページ {id} は削除されているので作り直しません")),
                },
                Some(None) => Plan::Skip("前に見つからなかったので作りません".into()),
                // 記録に無い＝今の版で足されたサンプル
                None => match parent {
                    Some(None) => Plan::Skip("親のサンプルが無いので作りません".into()),
                    _ => Plan::Create,
                },
            },
            None => {
                let title = build(s, &dummy).2;
                let parent_id = match parent {
                    None => Some(None),
                    Some(Some(pid)) => Some(Some(pid)),
                    Some(None) => None,
                };
                match parent_id {
                    None => Plan::Skip("親のサンプルが見つからないので触れません".into()),
                    Some(pid) => {
                        let hits: Vec<&Existing> = pages
                            .iter()
                            .filter(|p| !p.deleted && p.parent_id == pid && p.title == title)
                            .collect();
                        match hits.as_slice() {
                            [p] => Plan::Update(p.id),
                            [] => Plan::Skip(format!("「{title}」が見つからないので触れません")),
                            _ => Plan::Skip(format!(
                                "「{title}」が {} 件あるのでどれか決められません",
                                hits.len()
                            )),
                        }
                    }
                }
            }
        };
        let id = match &plan {
            Plan::Update(id) => Some(*id),
            // 作るページの id はまだ無いが、子のサンプルは作ってよい
            Plan::Create => Some(0),
            Plan::Skip(_) => None,
        };
        resolved.insert(s.key, id);
        plans.push(plan);
    }
    plans
}

/// ブロックの id を除いた本文（作るたびに id が変わるので、中身が同じかを比べるため）
fn without_ids(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| k.as_str() != "id")
                .map(|(k, v)| (k.clone(), without_ids(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(without_ids).collect()),
        v => v.clone(),
    }
}

pub struct UpdateReport {
    pub lines: Vec<String>,
    pub updated: usize,
    pub created: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

/// 入っているサンプルを、今の版の内容で上書きする。`dry_run` なら何をするかだけを返す
pub async fn update(
    db: &DatabaseConnection,
    upload_dir: &Path,
    user_id: i32,
    dry_run: bool,
) -> anyhow::Result<UpdateReport> {
    let txn = db.begin().await?;
    let record = load_record(&txn).await?;
    let all: Vec<page::Model> = page::Entity::find().all(&txn).await?;
    let existing: Vec<Existing> = all
        .iter()
        .map(|p| Existing {
            id: p.id,
            parent_id: p.parent_id,
            title: p.title.clone(),
            deleted: p.deleted_at.is_some(),
        })
        .collect();
    let specs = specs();
    let plans = make_plan(&specs, record.as_ref(), &existing);

    // 画像：記録の画像（記録が無ければ seed が置いた画像）がまだあれば使い回す
    let candidates: Vec<String> = match record.as_ref().and_then(|r| r.banner.clone()) {
        Some(f) => upload::Entity::find()
            .filter(upload::Column::Filename.eq(&f))
            .all(&txn)
            .await?
            .into_iter()
            .map(|u| u.filename)
            .collect(),
        None => upload::Entity::find()
            .filter(upload::Column::OriginalName.eq("banner.png"))
            .filter(upload::Column::UploadedBy.is_null())
            .order_by_desc(upload::Column::Id)
            .all(&txn)
            .await?
            .into_iter()
            .map(|u| u.filename)
            .collect(),
    };
    let mut banner = None;
    for f in candidates {
        if tokio::fs::try_exists(upload_dir.join(&f))
            .await
            .unwrap_or(false)
        {
            banner = Some(f);
            break;
        }
    }
    if banner.is_none() && !dry_run && plans.iter().any(|p| !matches!(p, Plan::Skip(_))) {
        banner = Some(put_banner(&txn, upload_dir).await?);
    }
    let mut ctx = Ctx {
        ids: HashMap::new(),
        banner: format!("/uploads/{}", banner.clone().unwrap_or_default()),
    };

    // 1. 作るページを先に作って id を決める（ページリンクに使うため）
    for (s, plan) in specs.iter().zip(&plans) {
        match plan {
            Plan::Update(id) => {
                ctx.ids.insert(s.key, *id);
            }
            Plan::Create if !dry_run => {
                let parent = s.parent.map(|k| ctx.ids[k]);
                let order = next_order(&txn, parent).await?;
                let id = insert_empty(&txn, s, parent, order).await?;
                ctx.ids.insert(s.key, id);
            }
            _ => {}
        }
    }

    // 2. 本文を入れる
    let mut report = UpdateReport {
        lines: Vec::new(),
        updated: 0,
        created: 0,
        unchanged: 0,
        skipped: 0,
    };
    let now = Utc::now();
    for (s, plan) in specs.iter().zip(&plans) {
        let label = if s.title.is_empty() {
            format!("{}（{}）", s.key, build(s, &ctx).2)
        } else {
            format!("{}（{}）", s.key, s.title)
        };
        match plan {
            Plan::Skip(why) => {
                report.skipped += 1;
                report.lines.push(format!("スキップ  {label}：{why}"));
            }
            Plan::Create => {
                report.created += 1;
                match ctx.ids.get(s.key) {
                    Some(&id) => {
                        fill(&txn, id, s, &ctx).await?;
                        report
                            .lines
                            .push(format!("作成      {label} → ページ {id}"));
                    }
                    None => report.lines.push(format!("作成      {label}")),
                }
            }
            Plan::Update(id) => {
                let p = all
                    .iter()
                    .find(|p| p.id == *id)
                    .expect("計画に入れたページ");
                let (content, plain, title) = build(s, &ctx);
                let old: Value = serde_json::from_str(&p.content).unwrap_or(Value::Null);
                if without_ids(&old) == without_ids(&content)
                    && p.title == title
                    && p.icon == s.icon
                {
                    report.unchanged += 1;
                    report.lines.push(format!("変更なし  {label}：ページ {id}"));
                    continue;
                }
                report.updated += 1;
                report.lines.push(format!("上書き    {label}：ページ {id}"));
                if dry_run {
                    continue;
                }
                // 今の状態が履歴に無ければ先に残す（seed で作ったままのページには版が無い）
                let latest = page_revision::Entity::find()
                    .filter(page_revision::Column::PageId.eq(p.id))
                    .order_by_desc(page_revision::Column::Id)
                    .one(&txn)
                    .await?;
                let in_history = latest.is_some_and(|r| {
                    r.content == p.content && r.title == p.title && r.icon == p.icon
                });
                if !in_history {
                    page_revision::ActiveModel {
                        page_id: Set(p.id),
                        title: Set(p.title.clone()),
                        icon: Set(p.icon.clone()),
                        content: Set(p.content.clone()),
                        created_by: Set(p.updated_by.or(p.created_by).or(Some(user_id))),
                        started_at: Set(p.updated_at),
                        updated_at: Set(p.updated_at),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                }
                let mut am: page::ActiveModel = p.clone().into();
                am.title = Set(title.clone());
                am.icon = Set(s.icon.into());
                am.content = Set(content.to_string());
                am.search_text = Set(Some(plain));
                am.version = Set(p.version + 1);
                am.updated_by = Set(Some(user_id));
                am.updated_at = Set(now);
                am.update(&txn).await?;
                pages::replace_links(&txn, p.id, &content)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                page_revision::ActiveModel {
                    page_id: Set(p.id),
                    title: Set(title),
                    icon: Set(s.icon.into()),
                    content: Set(content.to_string()),
                    created_by: Set(Some(user_id)),
                    started_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&txn)
                .await?;
            }
        }
    }

    if dry_run {
        txn.rollback().await?;
        return Ok(report);
    }
    // 3. 記録を今の状態にする。見つからなかった・削除されたサンプルは null にして次も作らない。
    //    ゴミ箱にあるものは番号を残す（ゴミ箱から戻せば、次からまた上書きの対象になる）
    let in_trash = |key: &str| {
        let id = record.as_ref()?.pages.get(key).copied().flatten()?;
        existing
            .iter()
            .any(|p| p.id == id && p.deleted)
            .then_some(id)
    };
    let record = Record {
        pages: specs
            .iter()
            .map(|s| {
                let id = ctx.ids.get(s.key).copied().or_else(|| in_trash(s.key));
                (s.key.to_string(), id)
            })
            .collect(),
        banner,
    };
    save_record(&txn, &record).await?;
    txn.commit().await?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            ids: HashMap::from([("guide", 7)]),
            banner: "/uploads/x.png".into(),
        }
    }

    #[test]
    fn parses_inline_markup() {
        let v = inline("a **b** `c` ~~d~~ [e](https://x) {{guide}} f", &ctx());
        let kinds: Vec<String> = v
            .iter()
            .map(|x| {
                let s = &x["styles"];
                match x["type"].as_str().unwrap() {
                    "text" if s["bold"] == true => format!("B:{}", x["text"].as_str().unwrap()),
                    "text" if s["code"] == true => format!("C:{}", x["text"].as_str().unwrap()),
                    "text" if s["strike"] == true => format!("S:{}", x["text"].as_str().unwrap()),
                    "text" => format!("T:{}", x["text"].as_str().unwrap()),
                    "link" => format!("L:{}", x["href"].as_str().unwrap()),
                    "pageLink" => format!("P:{}", x["props"]["pageId"].as_str().unwrap()),
                    k => k.to_string(),
                }
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "T:a ",
                "B:b",
                "T: ",
                "C:c",
                "T: ",
                "S:d",
                "T: ",
                "L:https://x",
                "T: ",
                "P:7",
                "T: f"
            ]
        );
        // 閉じていない記号はそのまま文字として残す
        assert_eq!(inline("2 ** 3 と `x", &ctx())[0]["text"], "2 ** 3 と `x");
    }

    #[test]
    fn every_sample_page_builds() {
        let c = Ctx {
            ids: specs()
                .iter()
                .enumerate()
                .map(|(i, s)| (s.key, i as i32 + 1))
                .collect(),
            banner: "/uploads/x.png".into(),
        };
        for s in specs() {
            let v = json!((s.body)(&c));
            assert!(
                crate::sanitize::sanitize_blocks(v.clone()).is_ok(),
                "{}",
                s.key
            );
            assert!(!plaintext::extract(&v).is_empty(), "{}", s.key);
            // ページリンクの参照先がすべて定義済み（未定義なら 0 になる）
            assert!(!v.to_string().contains("\"pageId\":\"0\""), "{}", s.key);
        }
    }

    fn ex(id: i32, parent_id: Option<i32>, title: &str, deleted: bool) -> Existing {
        Existing {
            id,
            parent_id,
            title: title.into(),
            deleted,
        }
    }

    /// 記録の無い DB：タイトルと親子関係で探す
    #[test]
    fn plan_without_record_matches_by_title() {
        let specs = specs();
        let pages = vec![
            ex(1, None, "BrikRS へようこそ", false),
            ex(2, Some(1), "使い方ガイド", false),
            ex(3, Some(1), "書式サンプル", false),
            // 仕様と設計はゴミ箱、運用ガイドは消した
            ex(4, Some(1), "仕様と設計", true),
            ex(9, Some(1), "自分で足したページ", false),
            // 同じタイトルが最上位に2つ
            ex(20, None, "つぶやき", false),
            ex(21, None, "つぶやき", false),
            ex(30, None, "下書きのサンプル", false),
        ];
        let plans = make_plan(&specs, None, &pages);
        let by: HashMap<&str, &Plan> = specs.iter().map(|s| s.key).zip(&plans).collect();
        assert_eq!(by["welcome"], &Plan::Update(1));
        assert_eq!(by["guide"], &Plan::Update(2));
        assert_eq!(by["format"], &Plan::Update(3));
        assert!(matches!(by["spec"], Plan::Skip(_)));
        assert!(matches!(by["admin"], Plan::Skip(_)));
        assert!(matches!(by["timeline"], Plan::Skip(_)));
        // 親が決まらなければ投稿にも触れない
        assert!(matches!(by["post1"], Plan::Skip(_)));
        assert_eq!(by["draft"], &Plan::Update(30));
        assert!(!plans.contains(&Plan::Update(9)));
    }

    /// 記録のある DB：記録どおりに当て、消したものは作り直さず、記録に無いサンプルは作る
    #[test]
    fn plan_with_record() {
        let specs = specs();
        let mut record = Record::default();
        for (i, s) in specs.iter().enumerate() {
            record.pages.insert(s.key.into(), Some(i as i32 + 1));
        }
        record.pages.insert("draft".into(), None);
        record.pages.remove("admin");
        let mut pages: Vec<Existing> = specs
            .iter()
            .enumerate()
            // 記録で当てるので、タイトルを変えていてもよい
            .map(|(i, _)| ex(i as i32 + 1, None, "改名した", false))
            .collect();
        let guide = specs.iter().position(|s| s.key == "guide").unwrap();
        pages[guide].deleted = true;
        let format = specs.iter().position(|s| s.key == "format").unwrap() as i32 + 1;
        pages.retain(|p| p.id != format);
        let plans = make_plan(&specs, Some(&record), &pages);
        let by: HashMap<&str, &Plan> = specs.iter().map(|s| s.key).zip(&plans).collect();
        assert_eq!(by["welcome"], &Plan::Update(1));
        assert!(matches!(by["guide"], Plan::Skip(_)));
        assert!(matches!(by["format"], Plan::Skip(_)));
        assert_eq!(by["admin"], &Plan::Create);
        assert!(matches!(by["draft"], Plan::Skip(_)));
    }

    #[test]
    fn link_to_missing_sample_becomes_text() {
        let v = inline("{{draft}} を見る", &ctx());
        assert_eq!(v[0]["text"], "「下書きのサンプル」 を見る");
    }

    #[test]
    fn compares_without_block_ids() {
        let c = ctx();
        let a = json!(welcome(&Ctx {
            ids: specs().iter().map(|s| (s.key, 1)).collect(),
            banner: c.banner.clone()
        }));
        let b = json!(welcome(&Ctx {
            ids: specs().iter().map(|s| (s.key, 1)).collect(),
            banner: c.banner
        }));
        assert_ne!(a, b);
        assert_eq!(without_ids(&a), without_ids(&b));
    }
}
