//! API 以外の公開ページ：Atom フィードと、共有用のタイトル・説明（OGP）を埋め込んだ HTML。
//!
//! どちらもログインしていない閲覧者として扱う（下書きとその子孫は出さない）。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde_json::{Value, json};

use crate::{
    access::Viewer,
    AppState,
    api::{self, pages},
    entity::page,
    error::{AppError, AppResult},
    refs,
};

const FEED_ENTRIES: u64 = 20;

fn site_name(st: &AppState) -> String {
    api::site::site_title(st)
}

/// フィードや OGP に入れる絶対 URL の土台。PUBLIC_URL が無ければリクエストから組み立てる
fn base_url(st: &AppState, headers: &HeaderMap) -> String {
    if let Ok(u) = std::env::var("PUBLIC_URL") {
        return u.trim_end_matches('/').to_string();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .filter(|h| {
            h.chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
        })
        .unwrap_or("localhost");
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .filter(|_| st.trust_proxy)
        .filter(|p| *p == "https" || *p == "http")
        .unwrap_or(if st.cookie_secure { "https" } else { "http" });
    format!("{proto}://{host}")
}

fn absolute(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        format!("{base}/{}", url.trim_start_matches('/'))
    }
}

/// HTML / XML の文字列・属性値として安全にする
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            // XML 1.0 で使えない制御文字は落とす
            c if c.is_control() && !matches!(c, '\n' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}

fn display_title(t: &str) -> &str {
    if t.trim().is_empty() { "無題" } else { t }
}

// ---------- Atom フィード ----------

struct Entry {
    id: i32,
    title: String,
    summary: String,
    published: DateTime<Utc>,
    updated: DateTime<Utc>,
    author: Option<String>,
}

fn atom(base: &str, title: &str, alternate: &str, self_url: &str, entries: &[Entry]) -> Response {
    let updated = entries
        .iter()
        .map(|e| e.updated)
        .max()
        .unwrap_or_else(Utc::now);
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    x += "<feed xmlns=\"http://www.w3.org/2005/Atom\">\n";
    x += &format!("  <title>{}</title>\n", esc(title));
    x += &format!(
        "  <link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>\n",
        esc(alternate)
    );
    x += &format!(
        "  <link rel=\"self\" type=\"application/atom+xml\" href=\"{}\"/>\n",
        esc(self_url)
    );
    x += &format!("  <id>{}</id>\n", esc(self_url));
    x += &format!("  <updated>{}</updated>\n", updated.to_rfc3339());
    x += &format!("  <generator>BrikRS</generator>\n");
    for e in entries {
        let url = format!("{base}/p/{}", e.id);
        x += "  <entry>\n";
        x += &format!("    <title>{}</title>\n", esc(display_title(&e.title)));
        x += &format!(
            "    <link rel=\"alternate\" type=\"text/html\" href=\"{}\"/>\n",
            esc(&url)
        );
        x += &format!("    <id>{}</id>\n", esc(&url));
        x += &format!("    <published>{}</published>\n", e.published.to_rfc3339());
        x += &format!("    <updated>{}</updated>\n", e.updated.to_rfc3339());
        if let Some(a) = &e.author {
            x += &format!("    <author><name>{}</name></author>\n", esc(a));
        }
        x += &format!("    <summary type=\"text\">{}</summary>\n", esc(&e.summary));
        x += "  </entry>\n";
    }
    x += "</feed>\n";
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/atom+xml; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            ),
        ],
        x,
    )
        .into_response()
}

async fn to_entries(st: &AppState, rows: Vec<page::Model>) -> AppResult<Vec<Entry>> {
    let names = pages::user_names(&st.db, rows.iter().map(|p| p.created_by)).await?;
    Ok(rows
        .into_iter()
        .map(|p| Entry {
            id: p.id,
            summary: pages::snippet(p.search_text.as_deref().unwrap_or("")),
            author: p.created_by.and_then(|i| names.get(&i).cloned()),
            title: p.title,
            published: p.created_at,
            updated: p.updated_at,
        })
        .collect())
}

/// サイト全体：最近更新されたページ（タイムラインの投稿は各タイムラインのフィードで）
pub async fn site_feed(State(st): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let base = base_url(&st, &headers);
    let hidden = pages::excluded_from_recent(&st.db, &Viewer::Guest).await?;
    let mut q = page::Entity::find().filter(page::Column::DeletedAt.is_null());
    if !hidden.is_empty() {
        q = q.filter(page::Column::Id.is_not_in(hidden));
    }
    let rows = q
        .order_by_desc(page::Column::UpdatedAt)
        .limit(FEED_ENTRIES)
        .all(&st.db)
        .await?;
    let entries = to_entries(&st, rows).await?;
    let title = format!("{} 最近の更新", site_name(&st));
    Ok(atom(
        &base,
        &title,
        &format!("{base}/recent"),
        &format!("{base}/feed.atom"),
        &entries,
    ))
}

/// ページの子ページ（タイムラインの投稿）を新しい順に
pub async fn page_feed(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<Response> {
    let base = base_url(&st, &headers);
    let parent = pages::find_visible_page(&st.db, id, &Viewer::Guest).await?;
    let hidden = pages::hidden_for(&st.db, &Viewer::Guest).await?;
    let mut q = page::Entity::find()
        .filter(page::Column::ParentId.eq(id))
        .filter(page::Column::DeletedAt.is_null());
    if !hidden.is_empty() {
        q = q.filter(page::Column::Id.is_not_in(hidden));
    }
    let rows = q
        .order_by_desc(page::Column::Id)
        .limit(FEED_ENTRIES)
        .all(&st.db)
        .await?;
    let entries = to_entries(&st, rows).await?;
    let title = format!("{} - {}", display_title(&parent.title), site_name(&st));
    Ok(atom(
        &base,
        &title,
        &format!("{base}/p/{id}"),
        &format!("{base}/p/{id}/feed.atom"),
        &entries,
    ))
}

// ---------- サイトごとのアイコン ----------

/// サイトごとに差し替えられるファイル（BRANDING_DIR に同じ名前で置く）
pub const BRANDING_FILES: &[(&str, &str)] = &[
    ("favicon.ico", "image/x-icon"),
    ("icon-192.png", "image/png"),
    ("icon-512.png", "image/png"),
    ("apple-touch-icon.png", "image/png"),
    ("manifest.webmanifest", "application/manifest+json"),
];

/// /favicon.ico などのアイコン。BRANDING_DIR にあればそれを、無ければ同梱のもの（STATIC_DIR）を返す
pub async fn branding_file(st: AppState, name: &'static str, mime: &'static str) -> Response {
    for dir in [&st.branding_dir, &st.static_dir] {
        if let Ok(bytes) = tokio::fs::read(dir.join(name)).await {
            return (
                [
                    (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
                    // 差し替えたときにすぐ反映されるよう、キャッシュは短め（1時間）
                    (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600")),
                ],
                bytes,
            )
                .into_response();
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

// ---------- 共有用のタイトル・説明（OGP） ----------

/// index.html の <title> と、サイトのフィードの名前をサイトのタイトルにする。head を渡すとそれを <title> の代わりに入れる
fn render_index(st: &AppState, index: &str, head: Option<String>) -> Response {
    let site = site_name(st);
    let head = head.unwrap_or_else(|| format!("<title>{}</title>", esc(&site)));
    let html = index.replacen("<title>BrikRS</title>", &head, 1).replacen(
        "title=\"BrikRS 最近の更新\"",
        &format!("title=\"{} 最近の更新\"", esc(&site)),
        1,
    );
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        html,
    )
        .into_response()
}

/// SPA の画面（/p/{id} 以外のパスと "/"）
pub async fn spa_index(State(st): State<AppState>) -> Response {
    match st.index_html.clone() {
        Some(index) => render_index(&st, &index, None),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// /p/{id} の HTML。画面は SPA のままで、<head> にそのページのタイトルと説明を差し込む。
/// X などのリンクカードや検索エンジンは JS を実行しないので、ここで入れておく必要がある
pub async fn page_html(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Response {
    let Some(index) = st.index_html.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let head = match page_head(&st, &headers, id).await {
        Ok(h) => Some(h),
        // 見つからない・下書きのページは、ふつうの index.html を返して画面側で「見つかりません」を出す
        Err(AppError::NotFound) => None,
        Err(e) => {
            tracing::warn!("OGP の生成に失敗: {e:?}");
            None
        }
    };
    render_index(&st, &index, head)
}

async fn page_head(st: &AppState, headers: &HeaderMap, id: i32) -> AppResult<String> {
    let p = pages::find_visible_page(&st.db, id, &Viewer::Guest).await?;
    let base = base_url(st, headers);
    let site = site_name(st);
    let title = format!(
        "{}{}",
        if p.icon.is_empty() {
            String::new()
        } else {
            format!("{} ", p.icon)
        },
        display_title(&p.title)
    );
    let desc = pages::snippet(p.search_text.as_deref().unwrap_or(""));
    let url = format!("{base}/p/{id}");
    let content: Value = serde_json::from_str(&p.content).unwrap_or_else(|_| json!([]));
    // 本文に画像が無ければサイトのアイコンを使う（カードは小さい形にする）
    let image = refs::first_image(&content).map(|u| absolute(&base, &u));
    let card_image = image.clone().unwrap_or_else(|| format!("{base}/icon-512.png"));

    let mut h = format!("<title>{} - {}</title>\n", esc(&title), esc(&site));
    let meta = |prop: &str, val: &str| {
        format!(
            "    <meta property=\"{prop}\" content=\"{}\" />\n",
            esc(val)
        )
    };
    h += &format!(
        "    <meta name=\"description\" content=\"{}\" />\n",
        esc(&desc)
    );
    h += &format!("    <link rel=\"canonical\" href=\"{}\" />\n", esc(&url));
    h += &meta("og:site_name", &site);
    h += &meta("og:type", "article");
    h += &meta("og:title", &title);
    h += &meta("og:description", &desc);
    h += &meta("og:url", &url);
    h += &meta("og:image", &card_image);
    h += &format!(
        "    <meta name=\"twitter:card\" content=\"{}\" />\n",
        if image.is_some() {
            "summary_large_image"
        } else {
            "summary"
        }
    );
    if p.layout == "timeline" {
        h += &format!(
            "    <link rel=\"alternate\" type=\"application/atom+xml\" title=\"{}\" href=\"{}\" />\n",
            esc(&title),
            esc(&format!("{url}/feed.atom"))
        );
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup() {
        assert_eq!(
            esc("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
        assert_eq!(esc("a\u{0}b\u{1b}c\nd"), "abc\nd");
    }

    #[test]
    fn absolute_urls() {
        assert_eq!(
            absolute("https://w.example", "/uploads/a.png"),
            "https://w.example/uploads/a.png"
        );
        assert_eq!(
            absolute("https://w.example", "https://x/y.png"),
            "https://x/y.png"
        );
    }
}
