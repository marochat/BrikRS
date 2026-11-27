//! `brikrs page ...`：サーバー上（SSH 越し）からページを投稿・読み出すコマンド。
//!
//! DB に直接つなぐので認証はしない（`create-user` などと同じく、コンテナで実行できる人は管理者とみなす）。
//! Claude Code などからの自動投稿を想定し、本文は Markdown で受け渡す。

use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
};

use anyhow::{Context, bail};
use sea_orm::{DatabaseConnection, EntityTrait, QueryFilter, ColumnTrait, QueryOrder};
use serde_json::{Value, json};

use crate::{
    access,
    api::{self, pages},
    entity::{page, user_group},
    markdown,
};

pub const USAGE: &str = "\
使い方:
  brikrs page list [--json]                     ページの一覧（ツリー順。ゴミ箱は除く、下書き・公開範囲を限ったページも含む）
  brikrs page get <id> [--json]                 ページを Markdown で表示（--json は BlockNote のデータのまま）
  brikrs page export [--json]                   全ページを JSON の配列で出力（本文は Markdown。--json は BlockNote のまま）
  brikrs page thread <id>                       タイムラインの投稿・返信を含むスレッド全体を Markdown で表示
  brikrs page post --user <username> [--parent <id> | --reply-to <id>] [--title <タイトル>] [--icon <絵文字>] [--draft]
                   [--members | --group <グループ名>...]
                                                標準入力の Markdown でページを作る。--title が無く、
                                                先頭が「# 見出し」ならそれをタイトルにする。
                                                --reply-to はタイムラインの投稿・返信への返信（その子ページとして作る。
                                                投稿・返信でない id はエラー）
                                                --members はログインしている人だけ、--group はそのグループの人
                                                （と管理者）だけに公開する（--group は複数指定できる）
  brikrs page move <id> --parent <id|root>      ページを別の親の下（末尾）へ移す。投稿を返信に付け替えるときなど
  brikrs page trash <id> --user <username>      ページを子ページ（返信）ごとゴミ箱へ移す（Web のゴミ箱から復元できる）";

pub async fn page(db: &DatabaseConnection, args: &[String]) -> anyhow::Result<()> {
    let json = args.iter().any(|a| a == "--json");
    match args.first().map(String::as_str) {
        Some("list") => list(db, json).await,
        Some("get") => {
            let id = args.get(1).and_then(|s| s.parse().ok()).context(USAGE)?;
            get(db, id, json).await
        }
        Some("export") => export(db, json).await,
        Some("thread") => {
            let id = args.get(1).and_then(|s| s.parse().ok()).context(USAGE)?;
            thread(db, id).await
        }
        Some("post") => post(db, &args[1..]).await,
        Some("move") => move_page(db, &args[1..]).await,
        Some("trash") => trash(db, &args[1..]).await,
        _ => {
            eprintln!("{USAGE}");
            bail!("不明なコマンドです")
        }
    }
}

/// 標準出力に書く。`| head` などでパイプが先に閉じられても異常終了にしない
fn emit(s: &str) -> anyhow::Result<()> {
    match std::io::stdout().lock().write_all(s.as_bytes()) {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(e.into()),
        _ => Ok(()),
    }
}

fn err(e: crate::error::AppError) -> anyhow::Error {
    anyhow::anyhow!("{e:?}")
}

async fn list(db: &DatabaseConnection, json: bool) -> anyhow::Result<()> {
    let all = pages::all_summaries(db).await.map_err(err)?;
    if json {
        return emit(&(serde_json::to_string_pretty(&all)? + "\n"));
    }
    let mut kids: HashMap<Option<i32>, Vec<&pages::PageSummary>> = HashMap::new();
    for s in &all {
        kids.entry(s.parent_id).or_default().push(s);
    }
    fn walk(
        parent: Option<i32>,
        depth: usize,
        kids: &HashMap<Option<i32>, Vec<&pages::PageSummary>>,
        out: &mut String,
    ) {
        for s in kids.get(&parent).into_iter().flatten() {
            let mut flags = Vec::new();
            if s.layout != "page" {
                flags.push(s.layout.as_str());
            }
            if s.is_draft {
                flags.push("下書き");
            }
            match s.visibility.as_str() {
                access::MEMBERS => flags.push("登録ユーザー"),
                access::GROUPS => flags.push("グループ"),
                _ => {}
            }
            if s.is_template {
                flags.push("テンプレート");
            }
            let flags = if flags.is_empty() { String::new() } else { format!("  [{}]", flags.join(", ")) };
            let icon = if s.icon.is_empty() { String::new() } else { format!("{} ", s.icon) };
            out.push_str(&format!("{:>5}  {}{icon}{}{flags}\n", s.id, "  ".repeat(depth), s.title));
            walk(Some(s.id), depth + 1, kids, out);
        }
    }
    let mut out = String::new();
    walk(None, 0, &kids, &mut out);
    emit(&out)
}

/// タイムラインの投稿・返信なら、タイムラインと元の投稿の id（元の投稿自身なら root = 自分）
struct ThreadInfo {
    timeline: i32,
    root: i32,
}

fn thread_info(all: &[pages::PageSummary], id: i32) -> Option<ThreadInfo> {
    let (timeline, chain) = pages::thread_position(all, id)?;
    Some(ThreadInfo { timeline, root: chain.last().copied().unwrap_or(id) })
}

fn front_matter(p: &page::Model, names: &HashMap<i32, String>, thread: Option<&ThreadInfo>) -> String {
    let q = |s: &str| Value::String(s.into()).to_string();
    let name = |id: Option<i32>| id.and_then(|i| names.get(&i)).map_or("null".into(), |n| q(n));
    let mut out = String::from("---\n");
    out += &format!("id: {}\n", p.id);
    out += &format!("title: {}\n", q(&p.title));
    out += &format!("parent_id: {}\n", p.parent_id.map_or("null".into(), |i| i.to_string()));
    if !p.icon.is_empty() {
        out += &format!("icon: {}\n", q(&p.icon));
    }
    if p.layout != "page" {
        out += &format!("layout: {}\n", p.layout);
    }
    if let Some(t) = thread {
        out += &format!("timeline: {}\n", t.timeline);
        if t.root != p.id {
            out += &format!("thread_root: {}\n", t.root);
        }
    }
    if p.is_draft {
        out += "draft: true\n";
    }
    if p.visibility != access::PUBLIC {
        out += &format!("visibility: {}\n", p.visibility);
    }
    if p.is_template {
        out += "template: true\n";
    }
    if let Some(s) = json_field(&p.db_schema) {
        out += &format!("db_schema: {s}\n");
    }
    if let Some(s) = json_field(&p.properties) {
        out += &format!("properties: {s}\n");
    }
    out += &format!("created_at: {}\n", p.created_at.to_rfc3339());
    out += &format!("created_by: {}\n", name(p.created_by));
    out += &format!("updated_at: {}\n", p.updated_at.to_rfc3339());
    out += &format!("updated_by: {}\n", name(p.updated_by));
    out += "---\n\n";
    out
}

fn content_of(p: &page::Model) -> Value {
    serde_json::from_str(&p.content).unwrap_or_else(|_| json!([]))
}

/// db_schema・properties（JSON文字列の列）をそのまま1行のJSONとして出す
fn json_field(s: &Option<String>) -> Option<String> {
    let v: Value = serde_json::from_str(s.as_deref()?).ok()?;
    Some(v.to_string())
}

async fn get(db: &DatabaseConnection, id: i32, json: bool) -> anyhow::Result<()> {
    let p = pages::find_live_page(db, id)
        .await
        .map_err(|_| anyhow::anyhow!("ページ {id} はありません（ゴミ箱に入っているかもしれません）"))?;
    if json {
        let d = pages::detail(db, p).await.map_err(err)?;
        return emit(&(serde_json::to_string_pretty(&d)? + "\n"));
    }
    let names = pages::user_names(db, [p.created_by, p.updated_by]).await.map_err(err)?;
    let all = pages::all_summaries(db).await.map_err(err)?;
    let thread = thread_info(&all, id);
    let titles = all.into_iter().map(|s| (s.id, s.title)).collect();
    let md = markdown::from_blocks(&content_of(&p), &titles);
    emit(&(front_matter(&p, &names, thread.as_ref()) + &md))
}

/// スレッド全体（元の投稿と、その下の返信すべて）を、投稿された順の木として Markdown で出す
async fn thread(db: &DatabaseConnection, id: i32) -> anyhow::Result<()> {
    let all = pages::all_summaries(db).await.map_err(err)?;
    let t = thread_info(&all, id).with_context(|| format!("ページ {id} はタイムラインの投稿・返信ではありません"))?;
    let pairs: Vec<(i32, Option<i32>)> = all.iter().map(|p| (p.id, p.parent_id)).collect();
    let ids: Vec<i32> = pages::subtree_levels(&pairs, t.root).into_iter().flatten().collect();
    let rows = page::Entity::find()
        .filter(page::Column::Id.is_in(ids))
        .filter(page::Column::DeletedAt.is_null())
        .order_by_asc(page::Column::Id)
        .all(db)
        .await?;
    let names = pages::user_names(db, rows.iter().map(|p| p.created_by)).await.map_err(err)?;
    let titles: HashMap<i32, String> = all.iter().map(|s| (s.id, s.title.clone())).collect();
    let mut kids: HashMap<Option<i32>, Vec<&page::Model>> = HashMap::new();
    for r in &rows {
        kids.entry(r.parent_id).or_default().push(r);
    }
    let timeline = titles.get(&t.timeline).map_or("", String::as_str);
    let mut out = format!("<!-- タイムライン {} 「{timeline}」 のスレッド。返信するには --reply-to <id> -->\n", t.timeline);
    let root = rows.iter().find(|r| r.id == t.root).context("元の投稿が見つかりません")?;
    let mut stack = vec![(root, 0usize)];
    while let Some((p, depth)) = stack.pop() {
        let author = p.created_by.and_then(|i| names.get(&i)).map_or("", String::as_str);
        let reply = if p.id == t.root { String::new() } else { format!(" · #{} への返信", p.parent_id.unwrap_or_default()) };
        let draft = if p.is_draft { " · 下書き" } else { "" };
        let mark = if p.id == id { " ◀" } else { "" };
        out += &format!(
            "\n{} #{} · {author} · {}{reply}{draft}{mark}\n\n",
            "#".repeat((depth + 2).min(6)),
            p.id,
            p.created_at.to_rfc3339(),
        );
        out += markdown::from_blocks(&content_of(p), &titles).trim_end();
        out += "\n";
        // 古い返信から順に出すので、逆順に積む
        for k in kids.get(&Some(p.id)).into_iter().flatten().rev() {
            stack.push((k, depth + 1));
        }
    }
    emit(&out)
}

async fn trash(db: &DatabaseConnection, args: &[String]) -> anyhow::Result<()> {
    let id: i32 = args.first().and_then(|s| s.parse().ok()).context(USAGE)?;
    let username = match (args.get(1).map(String::as_str), args.get(2)) {
        (Some("--user"), Some(u)) => u,
        _ => bail!("{USAGE}"),
    };
    let user = api::find_user(db, username)
        .await
        .map_err(err)?
        .with_context(|| format!("ユーザー {username} はいません"))?;
    let count = pages::trash_page(db, id, user.id).await.map_err(|e| match e {
        crate::error::AppError::BadRequest(m) => anyhow::anyhow!("{m}"),
        _ => anyhow::anyhow!("ページ {id} はありません（もうゴミ箱に入っているかもしれません）"),
    })?;
    println!("ページ {id} をゴミ箱へ移しました（子ページを含めて {count} 件）");
    Ok(())
}

async fn move_page(db: &DatabaseConnection, args: &[String]) -> anyhow::Result<()> {
    let id: i32 = args.first().and_then(|s| s.parse().ok()).context(USAGE)?;
    let parent_id = match (args.get(1).map(String::as_str), args.get(2).map(String::as_str)) {
        (Some("--parent"), Some("root")) => None,
        (Some("--parent"), Some(p)) => Some(p.parse().ok().context("--parent にはページの id か root を指定します")?),
        _ => bail!("{USAGE}"),
    };
    pages::move_to(db, id, parent_id, None).await.map_err(|e| match e {
        crate::error::AppError::BadRequest(m) => anyhow::anyhow!("移動できませんでした: {m}"),
        e => anyhow::anyhow!("移動できませんでした: {e:?}"),
    })?;
    match parent_id {
        Some(p) => println!("ページ {id} を {p} の下へ移しました"),
        None => println!("ページ {id} を最上位へ移しました"),
    }
    Ok(())
}

async fn export(db: &DatabaseConnection, json: bool) -> anyhow::Result<()> {
    let all = page::Entity::find()
        .filter(page::Column::DeletedAt.is_null())
        .order_by_asc(page::Column::Id)
        .all(db)
        .await?;
    let titles: HashMap<i32, String> = all.iter().map(|p| (p.id, p.title.clone())).collect();
    let names = pages::user_names(db, all.iter().flat_map(|p| [p.created_by, p.updated_by]))
        .await
        .map_err(err)?;
    let name = |id: Option<i32>| id.and_then(|i| names.get(&i).cloned());
    let out: Vec<Value> = all
        .iter()
        .map(|p| {
            let content = content_of(p);
            let mut v = json!({
                "id": p.id,
                "parent_id": p.parent_id,
                "title": p.title,
                "icon": p.icon,
                "layout": p.layout,
                "is_draft": p.is_draft,
                "is_template": p.is_template,
                "db_schema": p.db_schema.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
                "properties": p.properties.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
                "created_at": p.created_at,
                "created_by": name(p.created_by),
                "updated_at": p.updated_at,
                "updated_by": name(p.updated_by),
            });
            if json {
                v["content"] = content;
            } else {
                v["markdown"] = markdown::from_blocks(&content, &titles).into();
            }
            v
        })
        .collect();
    emit(&(serde_json::to_string_pretty(&out)? + "\n"))
}

/// 先頭の「# 見出し」をタイトルとして取り出す
fn split_title(md: &str) -> Option<(String, &str)> {
    let s = md.trim_start();
    let line_end = s.find('\n').unwrap_or(s.len());
    let title = s[..line_end].strip_prefix("# ")?.trim();
    Some((title.to_string(), &s[line_end..]))
}

async fn post(db: &DatabaseConnection, args: &[String]) -> anyhow::Result<()> {
    let mut username = None;
    let mut parent_id = None;
    let mut title = None;
    let mut icon = None;
    let mut is_draft = false;
    let mut visibility = access::PUBLIC;
    let mut group_names: Vec<String> = Vec::new();
    let mut reply = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--user" => username = it.next().cloned(),
            "--parent" => {
                parent_id = Some(it.next().and_then(|s| s.parse().ok()).context("--parent にはページの id を指定します")?)
            }
            "--reply-to" => {
                let id = it.next().and_then(|s| s.parse().ok()).context("--reply-to には投稿の id を指定します")?;
                parent_id = Some(id);
                reply = true;
            }
            "--title" => title = it.next().cloned(),
            "--icon" => icon = it.next().cloned(),
            "--draft" => is_draft = true,
            "--members" => visibility = access::MEMBERS,
            "--group" => {
                visibility = access::GROUPS;
                group_names.push(it.next().cloned().context("--group にはグループ名を指定します")?);
            }
            _ => bail!("{USAGE}"),
        }
    }
    let username = username.context("--user で投稿者のユーザー名を指定してください")?;
    let user = api::find_user(db, &username)
        .await
        .map_err(err)?
        .with_context(|| format!("ユーザー {username} はいません"))?;

    if let (true, Some(pid)) = (reply, parent_id) {
        let all = pages::all_summaries(db).await.map_err(err)?;
        let is_timeline = all.iter().any(|s| s.id == pid && s.layout == "timeline");
        // id の打ち間違いで関係ないページの下に作らないよう、返信先でなければ止める
        if is_timeline {
            bail!("ページ {pid} はタイムラインです。新しい投稿にするなら --parent {pid} を指定してください");
        } else if thread_info(&all, pid).is_none() {
            bail!("ページ {pid} はタイムラインの投稿・返信ではありません（子ページを作るなら --parent を指定してください）");
        }
    }

    let mut src = String::new();
    std::io::stdin().read_to_string(&mut src)?;
    let (title, body) = match title {
        Some(t) => (t, src.as_str()),
        None => split_title(&src).map_or((String::new(), src.as_str()), |(t, b)| (t, b)),
    };
    let content = markdown::to_blocks(body);
    if title.trim().is_empty() && content.as_array().is_none_or(Vec::is_empty) {
        bail!("タイトルも本文もありません");
    }

    let mut group_ids = Vec::new();
    for name in &group_names {
        let g = user_group::Entity::find()
            .filter(user_group::Column::Name.eq(name))
            .one(db)
            .await?
            .with_context(|| format!("グループ {name} はありません"))?;
        group_ids.push(g.id);
    }

    let p = pages::create_page(
        db,
        &access::Viewer::cli(user.id),
        pages::CreateReq {
            parent_id,
            title,
            content: Some(content),
            is_draft,
            icon,
            layout: None,
            db_schema: None,
            from_template: false,
            visibility: Some(visibility.into()),
            group_ids,
        },
    )
    .await
    .map_err(err)?;
    let base = std::env::var("PUBLIC_URL").unwrap_or_default();
    println!("作成しました: id={} {}/p/{}", p.id, base.trim_end_matches('/'), p.id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_title_from_first_heading() {
        let (t, b) = split_title("\n# タイトル\n\n本文\n").unwrap();
        assert_eq!(t, "タイトル");
        assert_eq!(b, "\n\n本文\n");
        assert!(split_title("## 小見出し\n").is_none());
        assert!(split_title("本文\n").is_none());
    }

    fn sample_page() -> page::Model {
        let now = chrono::Utc::now();
        page::Model {
            id: 1,
            parent_id: None,
            title: "DB".into(),
            icon: String::new(),
            content: "[]".into(),
            sort_order: 0,
            created_by: None,
            updated_by: None,
            created_at: now,
            updated_at: now,
            search_text: None,
            deleted_at: None,
            deleted_by: None,
            trash_root_id: None,
            version: 1,
            is_draft: false,
            visibility: "public".into(),
            layout: "database".into(),
            is_pinned: false,
            db_schema: Some(r#"[{"id":"c1","kind":"url"}]"#.into()),
            properties: Some(r#"{"c1":"https://example.com"}"#.into()),
            is_template: true,
            allow_guest_comments: false,
            allow_reactions: false,
            is_locked: false,
        }
    }

    #[test]
    fn front_matter_includes_template_schema_and_properties() {
        let fm = front_matter(&sample_page(), &HashMap::new(), None);
        assert!(fm.contains("template: true\n"), "{fm}");
        assert!(fm.contains(r#"db_schema: [{"id":"c1","kind":"url"}]"#), "{fm}");
        assert!(fm.contains(r#"properties: {"c1":"https://example.com"}"#), "{fm}");
    }
}
