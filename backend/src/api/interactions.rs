//! タイムラインの投稿へのゲストコメント・リアクションの「受け付けるか」の判定と、件数のまとめ、リアクションの API。
//!
//! 受け付けるのは、次をすべて満たすときだけ：
//! - サイト設定（管理画面のサイトタブ）で ON
//! - 一番近い親のタイムラインのページで ON（表示形式を timeline 以外にすると止まる。データは消さない）
//! - 付ける先が、付ける人に見える（ゴミ箱は誰にも見えない）。ゲストコメントは、ログインしていない人に
//!   見える（下書きでも、公開範囲を限ってもいない）タイムラインの直下の投稿だけ。
//!   リアクションは返信にも、見える人なら下書き・公開範囲を限った投稿にも付けられる

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::SocketAddr,
    sync::OnceLock,
    time::Duration,
};

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set, SqlErr,
    sea_query::{Expr, ExprTrait},
};
use serde::Serialize;

use crate::{
    AppState,
    access::{self, Viewer},
    auth::{self, MaybeUser},
    entity::{comment, page, reaction},
    error::{AppError, AppResult},
    guest::{self, WindowLimiter},
};

use super::{pages, site};

/// リアクションの種類（絵文字との対応はフロントエンドに置く）
pub const KINDS: &[&str] = &["like", "heart", "laugh", "wow", "party"];
/// 1つの IP から1時間に付け外しできる回数
const REACTIONS_PER_HOUR: u32 = 60;
/// 1つの投稿・1つの IP で数える訪問者の上限（Cookie を消しての水増しを抑える）
const VISITORS_PER_IP_PER_POST: u64 = 10;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Policy {
    /// ゲストコメントを受け付ける（表示もする）
    pub comments: bool,
    /// リアクションを受け付ける（表示もする）
    pub reactions: bool,
    /// ゲストコメントに CAPTCHA を付ける
    pub captcha: bool,
}

/// タイムラインのページの設定とサイト設定から
pub fn policy_of(st: &AppState, timeline: &page::Model) -> Policy {
    let s = site::current(st);
    let is_timeline = timeline.layout == "timeline";
    Policy {
        comments: is_timeline && s.guest_comments_on() && timeline.allow_guest_comments,
        reactions: is_timeline && s.reactions_on() && timeline.allow_reactions,
        captcha: s.comment_captcha_on(),
    }
}

/// ゲストが書き込もうとしている先
pub struct Target {
    pub policy: Policy,
    /// タイムラインの直下の投稿か（返信なら false）
    pub top_level: bool,
}

/// viewer に見えるタイムラインの投稿・返信なら、その受け付けの設定。そうでなければ 404
/// （見えない人には、ページがあることも知らせない）。ゲストコメントは Viewer::Guest で呼ぶ
pub async fn target(st: &AppState, page_id: i32, viewer: &Viewer) -> AppResult<Target> {
    pages::find_visible_page(&st.db, page_id, viewer).await?;
    let all = pages::visible_summaries(&st.db, viewer).await?;
    let (timeline_id, chain) = pages::thread_position(&all, page_id).ok_or(AppError::NotFound)?;
    let timeline = page::Entity::find_by_id(timeline_id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Target { policy: policy_of(st, &timeline), top_level: chain.is_empty() })
}

/// 投稿・返信ごとの件数
#[derive(Debug, Default, Serialize)]
pub struct ItemStats {
    /// 種類 → 件数（0 件の種類は入れない）
    reactions: BTreeMap<String, u64>,
    /// この訪問者が付けた種類
    mine: Vec<String>,
    /// 公開中のコメントの数（返信も含む。直下の投稿だけ）
    comments: u64,
}

#[derive(Debug, Serialize)]
pub struct Interactions {
    #[serde(flatten)]
    policy: Policy,
    /// 投稿・返信の id → 件数（何も無いものは入れない）
    items: HashMap<i32, ItemStats>,
    /// ゲストコメントを受け付けない投稿（ログインしていない人に見えない下書きなど）。コメント欄を出さない
    no_guest_comments: Vec<i32>,
}

/// ids の件数をまとめて数える（受け付けていないものは数えない）。top_level はコメントを数える投稿、
/// guest_hidden はログインしていない人に見えないページ（そこへはゲストコメントを受け付けない）
pub async fn collect(
    db: &impl ConnectionTrait,
    jar: &CookieJar,
    policy: Policy,
    ids: &[i32],
    top_level: &HashSet<i32>,
    guest_hidden: &HashSet<i32>,
) -> AppResult<Interactions> {
    let mut items: HashMap<i32, ItemStats> = HashMap::new();
    let mut no_guest_comments: Vec<i32> = top_level.intersection(guest_hidden).copied().collect();
    no_guest_comments.sort_unstable();
    let top_level: HashSet<i32> = top_level.difference(guest_hidden).copied().collect();
    if policy.reactions && !ids.is_empty() {
        let counts: Vec<(i32, String, i64)> = reaction::Entity::find()
            .select_only()
            .column(reaction::Column::PageId)
            .column(reaction::Column::Kind)
            .column_as(Expr::col(reaction::Column::VisitorHash).count(), "n")
            .filter(reaction::Column::PageId.is_in(ids.to_vec()))
            .group_by(reaction::Column::PageId)
            .group_by(reaction::Column::Kind)
            .into_tuple()
            .all(db)
            .await?;
        for (id, kind, n) in counts {
            if KINDS.contains(&kind.as_str()) {
                items.entry(id).or_default().reactions.insert(kind, n as u64);
            }
        }
        if let Some(h) = guest::visitor_hash(jar) {
            let mine: Vec<(i32, String)> = reaction::Entity::find()
                .select_only()
                .column(reaction::Column::PageId)
                .column(reaction::Column::Kind)
                .filter(reaction::Column::PageId.is_in(ids.to_vec()))
                .filter(reaction::Column::VisitorHash.eq(h))
                .into_tuple()
                .all(db)
                .await?;
            for (id, kind) in mine {
                items.entry(id).or_default().mine.push(kind);
            }
        }
    }
    if policy.comments && !top_level.is_empty() {
        let counts: Vec<(i32, i64)> = comment::Entity::find()
            .select_only()
            .column(comment::Column::PageId)
            .column_as(Expr::col(comment::Column::Id).count(), "n")
            .filter(comment::Column::PageId.is_in(top_level.iter().copied().collect::<Vec<_>>()))
            .filter(comment::Column::Status.eq("approved"))
            .group_by(comment::Column::PageId)
            .into_tuple()
            .all(db)
            .await?;
        for (id, n) in counts {
            items.entry(id).or_default().comments = n as u64;
        }
    }
    Ok(Interactions { policy, items, no_guest_comments })
}

fn limiter() -> &'static WindowLimiter {
    static L: OnceLock<WindowLimiter> = OnceLock::new();
    L.get_or_init(|| WindowLimiter::new(REACTIONS_PER_HOUR, Duration::from_secs(60 * 60)))
}

/// リアクションの付け外しの前の確認（共通）
async fn prepare(
    st: &AppState,
    headers: &HeaderMap,
    addr: SocketAddr,
    id: i32,
    kind: &str,
    viewer: &Viewer,
) -> AppResult<String> {
    guest::check_same_origin(headers)?;
    if !KINDS.contains(&kind) {
        return Err(AppError::BadRequest("不明なリアクションです".into()));
    }
    if !target(st, id, viewer).await?.policy.reactions {
        return Err(AppError::BadRequest("この投稿にはリアクションを付けられません".into()));
    }
    let ip = auth::client_ip(addr, headers, st.trust_proxy);
    if !limiter().allow(&ip) {
        return Err(AppError::TooManyRequests);
    }
    Ok(ip)
}

async fn stats_of(
    st: &AppState,
    jar: &CookieJar,
    id: i32,
    viewer: &Viewer,
) -> AppResult<Json<Interactions>> {
    let policy = target(st, id, viewer).await?.policy;
    Ok(Json(
        collect(&st.db, jar, policy, &[id], &HashSet::new(), &HashSet::new()).await?,
    ))
}

/// リアクションを付ける（同じ種類を何度送っても1回分）。訪問者 Cookie が無ければ発行する
pub async fn add(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Path((id, kind)): Path<(i32, String)>,
) -> AppResult<(CookieJar, Json<Interactions>)> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let ip = prepare(&st, &headers, addr, id, &kind, &v).await?;
    let (jar, visitor) = guest::ensure_visitor(&st, jar);
    let mine = reaction::Entity::find()
        .filter(reaction::Column::PageId.eq(id))
        .filter(reaction::Column::VisitorHash.eq(&visitor))
        .count(&st.db)
        .await?;
    if mine == 0 {
        // この訪問者がこの投稿で初めてなら、同じ IP からの訪問者の数を確かめる
        let others: Vec<String> = reaction::Entity::find()
            .select_only()
            .column(reaction::Column::VisitorHash)
            .distinct()
            .filter(reaction::Column::PageId.eq(id))
            .filter(reaction::Column::Ip.eq(&ip))
            .limit(VISITORS_PER_IP_PER_POST)
            .into_tuple()
            .all(&st.db)
            .await?;
        if others.len() as u64 >= VISITORS_PER_IP_PER_POST {
            return Err(AppError::TooManyRequests);
        }
    }
    let res = reaction::ActiveModel {
        page_id: Set(id),
        kind: Set(kind),
        visitor_hash: Set(visitor),
        ip: Set(Some(ip)),
        created_at: Set(Utc::now()),
    }
    .insert(&st.db)
    .await;
    match res {
        Ok(_) => {}
        // もう付けていた
        Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {}
        Err(e) => return Err(e.into()),
    }
    let body = stats_of(&st, &jar, id, &v).await?;
    Ok((jar, body))
}

/// リアクションを外す
pub async fn remove(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Path((id, kind)): Path<(i32, String)>,
) -> AppResult<Json<Interactions>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    prepare(&st, &headers, addr, id, &kind, &v).await?;
    if let Some(visitor) = guest::visitor_hash(&jar) {
        reaction::Entity::delete_many()
            .filter(reaction::Column::PageId.eq(id))
            .filter(reaction::Column::Kind.eq(kind))
            .filter(reaction::Column::VisitorHash.eq(visitor))
            .exec(&st.db)
            .await?;
    }
    stats_of(&st, &jar, id, &v).await
}
