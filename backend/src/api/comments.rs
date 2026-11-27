//! タイムラインの投稿へのゲストコメント（承認制）。
//!
//! - ゲストのコメントは必ず承認待ち（pending）で入り、管理者が承認する（approved）まで公開画面には出ない。
//!   却下はその場で削除する
//! - 本文はただのテキスト（リンクにも HTML にもしない）。名前は30文字、本文は1000文字まで
//! - 管理者は公開中のコメントに返信できる（すぐ公開。返信は1段まで）
//! - 投稿した人には削除用のトークンを返し、自分のコメントを消せるようにする
//! - 送信は、フォーム用トークン（CAPTCHA 付き、使い捨て）・ハニーポット・IP ごとの回数制限・承認待ちの総数の上限で守る
//! - 受け付けたら、設定があれば管理者へメールで知らせる（mail.rs。数分ごとにまとめて1通）

use std::{
    net::SocketAddr,
    sync::OnceLock,
    time::Duration,
};

use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
    sea_query::{Expr, Query as SqlQuery},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    access::Viewer,
    AppState,
    auth::{self, AdminUser},
    captcha,
    entity::{comment, page, user},
    error::{AppError, AppResult},
    guest::{self, WindowLimiter},
};

use super::interactions;

pub const NAME_MAX_CHARS: usize = 30;
pub const BODY_MAX_CHARS: usize = 1000;
const BODY_MAX_LINES: usize = 40;
/// 承認待ちがこれだけ溜まったら、新しいコメントを受け付けない（スパムで管理画面が埋まらないように）
const MAX_PENDING: u64 = 200;
const ADMIN_PAGE_SIZE: u64 = 50;
const BULK_MAX: usize = 100;

const PENDING: &str = "pending";
const APPROVED: &str = "approved";

/// フォームの読み込み（CAPTCHA の画像を作る）：IP ごとに10分で30回
fn token_limiter() -> &'static WindowLimiter {
    static L: OnceLock<WindowLimiter> = OnceLock::new();
    L.get_or_init(|| WindowLimiter::new(30, Duration::from_secs(10 * 60)))
}
/// 送信の試行（間違えたものも数える）：IP ごとに10分で10回
fn attempt_limiter() -> &'static WindowLimiter {
    static L: OnceLock<WindowLimiter> = OnceLock::new();
    L.get_or_init(|| WindowLimiter::new(10, Duration::from_secs(10 * 60)))
}
/// 受け付けたコメント：IP ごとに10分で3件
fn short_limiter() -> &'static WindowLimiter {
    static L: OnceLock<WindowLimiter> = OnceLock::new();
    L.get_or_init(|| WindowLimiter::new(3, Duration::from_secs(10 * 60)))
}
/// 受け付けたコメント：IP ごとに1日で20件
fn daily_limiter() -> &'static WindowLimiter {
    static L: OnceLock<WindowLimiter> = OnceLock::new();
    L.get_or_init(|| WindowLimiter::new(20, Duration::from_secs(24 * 60 * 60)))
}

// ---------- 入力の検査 ----------

/// 文字の向きを変える書式文字（表示の順序を入れ替えて、名前や本文を別物に見せられる）
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// 幅の無い・見えない書式文字（ZWJ は絵文字の組み合わせに使うので含めない。比べるときは name_key で除く）
fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}' | '\u{180E}' | '\u{200B}' | '\u{200C}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'
    )
}

/// 名前：前後の空白と制御文字・見えない書式文字を除き、連続する空白は1つにする
fn clean_name(s: &str) -> String {
    s.chars()
        // タブ・改行は空白として残し、下で1つの空白にまとめる
        .filter(|c| c.is_whitespace() || !(c.is_control() || is_bidi_control(*c) || is_invisible(*c)))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 本文：改行は \n にそろえ、改行以外の制御文字と文字の向きを変える書式文字を除く。
/// 3行以上の空行は2行にまとめ、前後の空白を除く
fn clean_body(s: &str) -> String {
    let s: String = s
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| (*c == '\n' || !c.is_control()) && !is_bidi_control(*c))
        .collect();
    let mut out = String::new();
    let mut blank = 0;
    for line in s.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

/// 本文に含まれる URL らしきものの数（管理画面で目印にする）
fn count_links(s: &str) -> usize {
    let lower = s.to_ascii_lowercase();
    ["http://", "https://", "www."]
        .iter()
        .map(|p| lower.matches(p).count())
        .sum::<usize>()
        // https://www. は1つと数える
        - lower.matches("://www.").count()
}

fn validate_body(body: &str) -> AppResult<()> {
    if body.is_empty() {
        return Err(AppError::BadRequest("本文を入力してください".into()));
    }
    if body.chars().count() > BODY_MAX_CHARS {
        return Err(AppError::BadRequest(format!("本文は {BODY_MAX_CHARS} 文字までです")));
    }
    if body.lines().count() > BODY_MAX_LINES {
        return Err(AppError::BadRequest(format!("本文は {BODY_MAX_LINES} 行までです")));
    }
    Ok(())
}

/// 比べるための形（大文字・小文字と空白の違い、見えない文字を無視する）。
/// ZWJ・異体字セレクタ・タグ文字は、英字に付けても見た目が変わらないので除く
fn name_key(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_whitespace()
                && !c.is_control()
                && !is_bidi_control(*c)
                && !is_invisible(*c)
                && !matches!(c, '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0000}'..='\u{E007F}')
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// ログインユーザーのユーザー名・表示名と同じ名前は、ゲストには使わせない（なりすまし防止）
async fn name_is_reserved(st: &AppState, name: &str) -> AppResult<bool> {
    let key = name_key(name);
    let users: Vec<(String, String)> = user::Entity::find()
        .select_only()
        .column(user::Column::Username)
        .column(user::Column::DisplayName)
        .into_tuple()
        .all(&st.db)
        .await?;
    Ok(users
        .iter()
        .any(|(u, d)| name_key(u) == key || name_key(d) == key))
}

// ---------- 公開の API ----------

#[derive(Deserialize)]
pub struct TokenQuery {
    page_id: i32,
}

/// フォーム用トークン（と CAPTCHA の画像）を発行する
pub async fn form_token(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
) -> AppResult<Json<Value>> {
    let t = interactions::target(&st, q.page_id, &Viewer::Guest).await?;
    if !t.policy.comments || !t.top_level {
        return Err(AppError::BadRequest("この投稿にはコメントできません".into()));
    }
    let ip = auth::client_ip(addr, &headers, st.trust_proxy);
    if !token_limiter().allow(&ip) {
        return Err(AppError::TooManyRequests);
    }
    let issued = captcha::issue(q.page_id, t.policy.captcha).await?;
    Ok(Json(json!({
        "token": issued.token,
        "image": issued.image_base64.map(|b| format!("data:image/png;base64,{b}")),
        "expires_at": issued.expires_at,
    })))
}

#[derive(Serialize)]
pub struct PublicComment {
    id: i32,
    name: String,
    body: String,
    created_at: DateTime<Utc>,
    /// 管理者の返信
    is_staff: bool,
    replies: Vec<PublicComment>,
}

impl PublicComment {
    fn from(c: comment::Model) -> Self {
        Self {
            id: c.id,
            name: c.author_name,
            body: c.body,
            created_at: c.created_at,
            is_staff: c.user_id.is_some(),
            replies: vec![],
        }
    }
}

/// 投稿の公開中のコメント（古い順）と、その下の管理者の返信。受け付けていなければ空
pub async fn list(State(st): State<AppState>, Path(id): Path<i32>) -> AppResult<Json<Value>> {
    let enabled = match interactions::target(&st, id, &Viewer::Guest).await {
        Ok(t) => t.policy.comments && t.top_level,
        // 閲覧者に見えない投稿（下書きなど）
        Err(AppError::NotFound) => false,
        Err(e) => return Err(e),
    };
    if !enabled {
        return Ok(Json(json!({ "enabled": false, "comments": [] })));
    }
    let rows = comment::Entity::find()
        .filter(comment::Column::PageId.eq(id))
        .filter(comment::Column::Status.eq(APPROVED))
        .order_by_asc(comment::Column::Id)
        .all(&st.db)
        .await?;
    Ok(Json(json!({ "enabled": true, "comments": thread(rows) })))
}

/// 返信を親の下に入れる（返信は1段まで。親が見えない返信は出さない）
fn thread(rows: Vec<comment::Model>) -> Vec<PublicComment> {
    let (tops, replies): (Vec<_>, Vec<_>) = rows.into_iter().partition(|c| c.parent_id.is_none());
    let mut out: Vec<PublicComment> = tops.into_iter().map(PublicComment::from).collect();
    for r in replies {
        if let Some(p) = out.iter_mut().find(|p| Some(p.id) == r.parent_id) {
            p.replies.push(PublicComment::from(r));
        }
    }
    out
}

#[derive(Deserialize)]
pub struct PostReq {
    name: String,
    body: String,
    token: String,
    #[serde(default)]
    captcha: String,
    /// ハニーポット（画面には出さない入力欄。何か入っていれば機械による送信）
    #[serde(default)]
    website: String,
}

/// ゲストのコメントを受け付ける（必ず承認待ち）
pub async fn create(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(req): Json<PostReq>,
) -> AppResult<(StatusCode, Json<Value>)> {
    guest::check_same_origin(&headers)?;
    let ip = auth::client_ip(addr, &headers, st.trust_proxy);
    if !attempt_limiter().allow(&ip) {
        return Err(AppError::TooManyRequests);
    }
    if !req.website.is_empty() {
        tracing::info!("guest comment from {ip}: honeypot");
        // 機械には、受け付けたように見せる
        return Ok((StatusCode::ACCEPTED, Json(json!({ "id": 0, "delete_token": "", "status": PENDING }))));
    }
    let t = interactions::target(&st, id, &Viewer::Guest).await?;
    if !t.policy.comments || !t.top_level {
        return Err(AppError::BadRequest("この投稿にはコメントできません".into()));
    }

    // 入力の誤りではトークンを使用済みにしない（直して送り直せるように）
    let name = clean_name(&req.name);
    let body = clean_body(&req.body);
    if name.is_empty() {
        return Err(AppError::BadRequest("名前を入力してください".into()));
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(AppError::BadRequest(format!("名前は {NAME_MAX_CHARS} 文字までです")));
    }
    validate_body(&body)?;
    if name_is_reserved(&st, &name).await? {
        return Err(AppError::BadRequest("その名前は使えません。別の名前にしてください".into()));
    }

    captcha::verify(&req.token, id, &req.captcha, t.policy.captcha)
        .map_err(|r| AppError::BadRequest(r.message().into()))?;

    if !short_limiter().check(&ip) || !daily_limiter().check(&ip) {
        return Err(AppError::TooManyRequests);
    }
    // 管理画面に出る分（ゴミ箱のページのものを除く）だけ数える。ゴミ箱の分まで数えると、管理画面に見えない承認待ちで受け付けが止まる
    let pending = comment::Entity::find()
        .filter(comment::Column::Status.eq(PENDING))
        .filter(on_live_pages())
        .count(&st.db)
        .await?;
    if pending >= MAX_PENDING {
        tracing::warn!("guest comment rejected: {pending} comments are waiting for approval");
        return Err(AppError::BadRequest(
            "ただいまコメントを受け付けられません。時間をおいてお試しください".into(),
        ));
    }

    let delete_token = auth::random_token();
    let c = comment::ActiveModel {
        page_id: Set(id),
        parent_id: Set(None),
        user_id: Set(None),
        author_name: Set(name),
        link_count: Set(count_links(&body).min(i16::MAX as usize) as i16),
        body: Set(body),
        status: Set(PENDING.into()),
        ip: Set(Some(ip.clone())),
        user_agent: Set(guest::user_agent(&headers)),
        delete_token_hash: Set(Some(auth::sha256_hex(&delete_token))),
        created_at: Set(Utc::now()),
        approved_at: Set(None),
        approved_by: Set(None),
        ..Default::default()
    }
    .insert(&st.db)
    .await?;
    short_limiter().hit(&ip);
    daily_limiter().hit(&ip);
    tracing::info!("guest comment {} on page {id} from {ip}", c.id);
    crate::mail::comment_received(&st);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "id": c.id,
            "delete_token": delete_token,
            "status": c.status,
            "name": c.author_name,
            "body": c.body,
            "created_at": c.created_at,
        })),
    ))
}

/// 投稿した人が自分のコメントを消す（ヘッダー X-Delete-Token に投稿時のトークン）
pub async fn delete_mine(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    guest::check_same_origin(&headers)?;
    let token = headers
        .get("x-delete-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let c = comment::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let ok = c.delete_token_hash.as_deref().is_some_and(|h| {
        auth::constant_time_eq(h.as_bytes(), auth::sha256_hex(token).as_bytes())
    });
    if !ok {
        return Err(AppError::Forbidden);
    }
    comment::Entity::delete_by_id(id).exec(&st.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MineItem {
    id: i32,
    token: String,
}

#[derive(Deserialize)]
pub struct MineReq {
    items: Vec<MineItem>,
}

const MINE_MAX: usize = 50;

/// 投稿した人が、手元に残した自分のコメント（id と削除用トークン）の今の状態を確かめる。
/// pending / approved。消えたもの（却下・削除）とトークンが合わないものは返さない
pub async fn mine(State(st): State<AppState>, Json(req): Json<MineReq>) -> AppResult<Json<Value>> {
    let items: Vec<&MineItem> = req.items.iter().take(MINE_MAX).collect();
    let rows = comment::Entity::find()
        .filter(comment::Column::Id.is_in(items.iter().map(|i| i.id).collect::<Vec<_>>()))
        .all(&st.db)
        .await?;
    let mut out = serde_json::Map::new();
    for c in rows {
        let Some(it) = items.iter().find(|i| i.id == c.id) else { continue };
        let ok = c.delete_token_hash.as_deref().is_some_and(|h| {
            auth::constant_time_eq(h.as_bytes(), auth::sha256_hex(&it.token).as_bytes())
        });
        if ok {
            out.insert(c.id.to_string(), Value::String(c.status));
        }
    }
    Ok(Json(Value::Object(out)))
}

// ---------- 管理者の API ----------

/// ゴミ箱に入っていないページに付いたコメントだけ（ゴミ箱のページのコメントは、完全に削除するとき一緒に消える）
fn on_live_pages() -> sea_orm::Condition {
    sea_orm::Condition::all().add(
        comment::Column::PageId.in_subquery(
            SqlQuery::select()
                .column(page::Column::Id)
                .from(page::Entity)
                .and_where(page::Column::DeletedAt.is_null())
                .to_owned(),
        ),
    )
}

#[derive(Deserialize)]
pub struct AdminListQuery {
    status: Option<String>,
    before: Option<i32>,
}

/// 管理画面のコメント一覧（新しい順）。status は pending（既定）/ approved
pub async fn admin_list(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Query(q): Query<AdminListQuery>,
) -> AppResult<Json<Value>> {
    let status = match q.status.as_deref() {
        None | Some("pending") => PENDING,
        Some("approved") => APPROVED,
        Some(_) => return Err(AppError::BadRequest("不明な状態です".into())),
    };
    let mut query = comment::Entity::find()
        .filter(comment::Column::Status.eq(status))
        .filter(on_live_pages());
    if let Some(b) = q.before {
        query = query.filter(comment::Column::Id.lt(b));
    }
    let mut rows = query
        .order_by_desc(comment::Column::Id)
        .limit(ADMIN_PAGE_SIZE + 1)
        .all(&st.db)
        .await?;
    let has_more = rows.len() as u64 > ADMIN_PAGE_SIZE;
    rows.truncate(ADMIN_PAGE_SIZE as usize);

    let page_ids: Vec<i32> = rows.iter().map(|c| c.page_id).collect();
    let titles: std::collections::HashMap<i32, (String, String)> = page::Entity::find()
        .select_only()
        .column(page::Column::Id)
        .column(page::Column::Title)
        .column(page::Column::Icon)
        .filter(page::Column::Id.is_in(page_ids))
        .into_tuple::<(i32, String, String)>()
        .all(&st.db)
        .await?
        .into_iter()
        .map(|(id, t, i)| (id, (t, i)))
        .collect();
    let items: Vec<Value> = rows
        .into_iter()
        .map(|c| {
            let (title, icon) = titles.get(&c.page_id).cloned().unwrap_or_default();
            json!({
                "id": c.id,
                "page_id": c.page_id,
                "page_title": title,
                "page_icon": icon,
                "parent_id": c.parent_id,
                "name": c.author_name,
                "body": c.body,
                "status": c.status,
                "is_staff": c.user_id.is_some(),
                "link_count": c.link_count,
                "ip": c.ip,
                "user_agent": c.user_agent,
                "created_at": c.created_at,
                "approved_at": c.approved_at,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items, "has_more": has_more })))
}

/// 承認待ちの件数（ナビバーのバッジ用）
pub async fn admin_count(State(st): State<AppState>, AdminUser(_): AdminUser) -> AppResult<Json<Value>> {
    let pending = comment::Entity::find()
        .filter(comment::Column::Status.eq(PENDING))
        .filter(on_live_pages())
        .count(&st.db)
        .await?;
    Ok(Json(json!({ "pending": pending })))
}

async fn approve_ids(st: &AppState, ids: Vec<i32>, admin_id: i32) -> AppResult<u64> {
    Ok(comment::Entity::update_many()
        .col_expr(comment::Column::Status, Expr::value(APPROVED))
        .col_expr(comment::Column::ApprovedAt, Expr::value(Utc::now()))
        .col_expr(comment::Column::ApprovedBy, Expr::value(admin_id))
        .filter(comment::Column::Id.is_in(ids))
        .filter(comment::Column::Status.eq(PENDING))
        .exec(&st.db)
        .await?
        .rows_affected)
}

pub async fn approve(
    State(st): State<AppState>,
    AdminUser(cu): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let n = approve_ids(&st, vec![id], cu.user.id).await?;
    Ok(Json(json!({ "approved": n })))
}

#[derive(Deserialize)]
pub struct BulkReq {
    ids: Vec<i32>,
    /// approve / delete
    action: String,
}

pub async fn bulk(
    State(st): State<AppState>,
    AdminUser(cu): AdminUser,
    Json(req): Json<BulkReq>,
) -> AppResult<Json<Value>> {
    if req.ids.is_empty() || req.ids.len() > BULK_MAX {
        return Err(AppError::BadRequest(format!("一度に選べるのは {BULK_MAX} 件までです")));
    }
    let n = match req.action.as_str() {
        "approve" => approve_ids(&st, req.ids, cu.user.id).await?,
        "delete" => {
            comment::Entity::delete_many()
                .filter(comment::Column::Id.is_in(req.ids))
                .exec(&st.db)
                .await?
                .rows_affected
        }
        _ => return Err(AppError::BadRequest("不明な操作です".into())),
    };
    Ok(Json(json!({ "count": n })))
}

/// 管理者がコメントを削除する（却下も同じ。返信も一緒に消える）
pub async fn admin_delete(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<StatusCode> {
    let r = comment::Entity::delete_by_id(id).exec(&st.db).await?;
    if r.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ReplyReq {
    body: String,
}

/// 管理者の返信（すぐ公開）。返信できるのは公開中のゲストのコメントだけ
pub async fn reply(
    State(st): State<AppState>,
    AdminUser(cu): AdminUser,
    Path(id): Path<i32>,
    Json(req): Json<ReplyReq>,
) -> AppResult<Json<PublicComment>> {
    let parent = comment::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if parent.parent_id.is_some() || parent.status != APPROVED {
        return Err(AppError::BadRequest("返信できるのは公開中のコメントだけです".into()));
    }
    let body = clean_body(&req.body);
    validate_body(&body)?;
    let now = Utc::now();
    let c = comment::ActiveModel {
        page_id: Set(parent.page_id),
        parent_id: Set(Some(parent.id)),
        user_id: Set(Some(cu.user.id)),
        author_name: Set(cu.user.display_name.chars().take(64).collect()),
        link_count: Set(count_links(&body).min(i16::MAX as usize) as i16),
        body: Set(body),
        status: Set(APPROVED.into()),
        ip: Set(None),
        user_agent: Set(None),
        delete_token_hash: Set(None),
        created_at: Set(now),
        approved_at: Set(Some(now)),
        approved_by: Set(Some(cu.user.id)),
        ..Default::default()
    }
    .insert(&st.db)
    .await?;
    Ok(Json(PublicComment::from(c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_input() {
        assert_eq!(clean_name("  まろ\t 茶\u{7}  "), "まろ 茶");
        assert_eq!(clean_body("\r\n a\r\n\r\n\r\n\r\nb  \u{0}\n"), "a\n\nb");
    }

    #[test]
    fn strips_invisible_and_bidi() {
        assert_eq!(clean_name("ad\u{200B}min\u{202E}"), "admin");
        assert_eq!(clean_name("\u{FEFF} まろ \u{2066}茶 "), "まろ 茶");
        assert_eq!(clean_name("a\tb\u{200B} c"), "a b c");
        // 絵文字の組み合わせ（ZWJ）は残す
        assert_eq!(clean_name("👩\u{200D}💻"), "👩\u{200D}💻");
        assert_eq!(clean_body("abc\u{202E}def\u{2067}"), "abcdef");
        // ZWJ・異体字セレクタを挟んでも、比べるときは同じ名前
        assert_eq!(name_key("ad\u{200D}mi\u{FE0F}n"), name_key("admin"));
        assert_eq!(name_key("A\u{E0041}dmin"), name_key("admin"));
    }

    #[test]
    fn counts_links() {
        assert_eq!(count_links("見て https://www.example.com と http://a.jp と www.b.jp"), 3);
        assert_eq!(count_links("リンクなし"), 0);
    }

    #[test]
    fn name_keys_ignore_case_and_spaces() {
        assert_eq!(name_key("Maro Cha"), name_key("marocha"));
        assert_ne!(name_key("maro"), name_key("marocha"));
    }

    #[test]
    fn body_limits() {
        assert!(validate_body("").is_err());
        assert!(validate_body(&"あ".repeat(BODY_MAX_CHARS)).is_ok());
        assert!(validate_body(&"あ".repeat(BODY_MAX_CHARS + 1)).is_err());
        assert!(validate_body(&"a\n".repeat(BODY_MAX_LINES + 1)).is_err());
    }

    fn row(id: i32, parent_id: Option<i32>) -> comment::Model {
        comment::Model {
            id,
            page_id: 1,
            parent_id,
            user_id: parent_id.map(|_| 1),
            author_name: format!("n{id}"),
            body: String::new(),
            status: APPROVED.into(),
            link_count: 0,
            ip: None,
            user_agent: None,
            delete_token_hash: None,
            created_at: Utc::now(),
            approved_at: None,
            approved_by: None,
        }
    }

    #[test]
    fn replies_go_under_parent() {
        let t = thread(vec![row(1, None), row(2, None), row(3, Some(1)), row(4, Some(99))]);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].replies.len(), 1);
        assert!(t[0].replies[0].is_staff);
        assert!(t[1].replies.is_empty());
    }
}
