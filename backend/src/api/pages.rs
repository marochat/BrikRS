use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    FromQueryResult, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use axum_extra::extract::cookie::CookieJar;

use super::interactions;
use crate::{
    AppState,
    access::{self, Viewer},
    auth::{AuthUser, MaybeUser},
    entity::{page, page_group, page_link, page_revision, user},
    error::{AppError, AppResult},
    plaintext, refs, sanitize,
};

const MAX_TITLE_CHARS: usize = 255;
const MAX_ICON_CHARS: usize = 8;
/// 同じ人の連続した編集を1つの版にまとめる時間幅
const REVISION_MERGE_WINDOW_MIN: i64 = 15;

/// ツリー表示用（本文なし）
#[derive(Serialize, FromQueryResult, Clone)]
pub struct PageSummary {
    pub id: i32,
    pub parent_id: Option<i32>,
    pub title: String,
    pub icon: String,
    pub sort_order: i32,
    pub is_draft: bool,
    /// 公開範囲（public / members / groups）
    pub visibility: String,
    /// 作った人（公開範囲の判定用。閲覧者には返さない）
    #[serde(skip)]
    pub created_by: Option<i32>,
    pub layout: String,
    pub is_template: bool,
    /// ロック（作った人と管理者以外は閲覧だけ。誰も移動・ゴミ箱にできない）
    pub is_locked: bool,
}

pub const LAYOUTS: &[&str] = &["page", "timeline", "database"];
const MAX_POSTS_PER_REQUEST: u64 = 20;
const RECENT_LIMIT: u64 = 30;
const AUTO_TITLE_CHARS: usize = 40;

const SUMMARY_COLUMNS: [page::Column; 11] = [
    page::Column::Id,
    page::Column::ParentId,
    page::Column::Title,
    page::Column::Icon,
    page::Column::SortOrder,
    page::Column::IsDraft,
    page::Column::Visibility,
    page::Column::CreatedBy,
    page::Column::Layout,
    page::Column::IsTemplate,
    page::Column::IsLocked,
];

/// ゴミ箱に入っていないページの一覧（公開範囲は見ない。サーバー内部・CLI 用）
pub async fn all_summaries(db: &impl ConnectionTrait) -> AppResult<Vec<PageSummary>> {
    Ok(page::Entity::find()
        .select_only()
        .columns(SUMMARY_COLUMNS)
        .filter(page::Column::DeletedAt.is_null())
        .order_by_asc(page::Column::SortOrder)
        .order_by_asc(page::Column::Id)
        .into_model::<PageSummary>()
        .all(db)
        .await?)
}

/// ゴミ箱のものも含めた全ページ（ゴミ箱の中のページが見えるかを、削除前の位置で判定する用）
pub async fn summaries_with_trash(db: &impl ConnectionTrait) -> AppResult<Vec<PageSummary>> {
    Ok(page::Entity::find()
        .select_only()
        .columns(SUMMARY_COLUMNS)
        .into_model::<PageSummary>()
        .all(db)
        .await?)
}

/// all のうち viewer に見えないページの id（グループの対応は必要なときだけ読む）
pub async fn hidden_in(
    db: &impl ConnectionTrait,
    all: &[PageSummary],
    viewer: &Viewer,
) -> AppResult<HashSet<i32>> {
    if viewer.is_admin() {
        return Ok(HashSet::new());
    }
    let pg = if viewer.is_guest() { HashMap::new() } else { access::page_groups(db).await? };
    Ok(access::hidden_ids(all, &pg, viewer))
}

/// viewer に見せてはいけないページの id（管理者なら空）
pub async fn hidden_for(db: &impl ConnectionTrait, viewer: &Viewer) -> AppResult<HashSet<i32>> {
    if viewer.is_admin() {
        return Ok(HashSet::new());
    }
    hidden_in(db, &all_summaries(db).await?, viewer).await
}

/// タイムラインの投稿とその返信（タイムライン表示のページの子孫）。最近の更新やサイトのフィードからは外す
pub fn timeline_post_ids(all: &[PageSummary]) -> HashSet<i32> {
    let pairs: Vec<(i32, Option<i32>)> = all.iter().map(|p| (p.id, p.parent_id)).collect();
    all.iter()
        .filter(|p| p.layout == "timeline")
        .flat_map(|t| subtree_levels(&pairs, t.id).into_iter().skip(1).flatten())
        .collect()
}

/// id がタイムラインの投稿・返信なら、そのタイムラインの id と、id から元の投稿までの祖先（近い順、id 自身は含まない）
pub fn thread_position(all: &[PageSummary], id: i32) -> Option<(i32, Vec<i32>)> {
    let by_id: HashMap<i32, &PageSummary> = all.iter().map(|p| (p.id, p)).collect();
    let mut chain = Vec::new();
    let mut cur = by_id.get(&id)?.parent_id;
    while let Some(c) = cur {
        let p = by_id.get(&c)?;
        if p.layout == "timeline" {
            return Some((c, chain));
        }
        // 循環していたら諦める（move_page で防いでいるが念のため）
        if chain.contains(&c) || chain.len() > all.len() {
            return None;
        }
        chain.push(c);
        cur = p.parent_id;
    }
    None
}

/// 最近の更新・サイトのフィードに出さないページ（閲覧者向けの非公開分と、タイムラインの投稿）
pub async fn excluded_from_recent(
    db: &impl ConnectionTrait,
    viewer: &Viewer,
) -> AppResult<HashSet<i32>> {
    let all = all_summaries(db).await?;
    let mut ex = timeline_post_ids(&all);
    ex.extend(hidden_in(db, &all, viewer).await?);
    Ok(ex)
}

/// 見てよいページの一覧（見えないページとその子孫を出さない）
pub async fn visible_summaries(
    db: &impl ConnectionTrait,
    viewer: &Viewer,
) -> AppResult<Vec<PageSummary>> {
    let all = all_summaries(db).await?;
    let hidden = hidden_in(db, &all, viewer).await?;
    Ok(all.into_iter().filter(|p| !hidden.contains(&p.id)).collect())
}

/// 見てよいページを取得（ゴミ箱のもの、viewer に見えないものは 404。あることも知らせない）
pub async fn find_visible_page(
    db: &impl ConnectionTrait,
    id: i32,
    viewer: &Viewer,
) -> AppResult<page::Model> {
    let p = find_live_page(db, id).await?;
    ensure_visible(db, id, viewer).await?;
    Ok(p)
}

/// ゴミ箱のページも含めて、viewer に見えなければ 404（削除する前の位置で判定する）
pub async fn ensure_visible_incl_trash(
    db: &impl ConnectionTrait,
    id: i32,
    viewer: &Viewer,
) -> AppResult<()> {
    if viewer.is_admin() {
        return Ok(());
    }
    let all = summaries_with_trash(db).await?;
    if hidden_in(db, &all, viewer).await?.contains(&id) {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// viewer に見えなければ 404
pub async fn ensure_visible(db: &impl ConnectionTrait, id: i32, viewer: &Viewer) -> AppResult<()> {
    if hidden_for(db, viewer).await?.contains(&id) {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
) -> AppResult<Json<Vec<PageSummary>>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    Ok(Json(visible_summaries(&st.db, &v).await?))
}

/// root と、parent_id を辿ってその下にある全ページの id を、浅い順の段ごとに返す
pub fn subtree_levels(pairs: &[(i32, Option<i32>)], root: i32) -> Vec<Vec<i32>> {
    let mut levels: Vec<Vec<i32>> = vec![vec![root]];
    loop {
        let prev = levels.last().unwrap();
        let next: Vec<i32> = pairs
            .iter()
            .filter(|(_, pid)| pid.is_some_and(|pid| prev.contains(&pid)))
            .map(|(id, _)| *id)
            .collect();
        if next.is_empty() {
            return levels;
        }
        levels.push(next);
    }
}

pub async fn user_names(
    db: &impl ConnectionTrait,
    ids: impl IntoIterator<Item = Option<i32>>,
) -> AppResult<HashMap<i32, String>> {
    let ids: Vec<i32> = ids.into_iter().flatten().collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(user::Entity::find()
        .filter(user::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.display_name))
        .collect())
}

#[derive(Serialize)]
pub struct PageDetail {
    id: i32,
    parent_id: Option<i32>,
    title: String,
    icon: String,
    content: Value,
    created_by: Option<String>,
    /// 作った人の id（公開範囲を変えられるかを画面で判断する用）
    created_by_id: Option<i32>,
    updated_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    version: i32,
    is_draft: bool,
    /// 公開範囲（public / members / groups）。このページ自身の設定（祖先の分は含まない）
    visibility: String,
    /// 公開範囲が groups のとき、見られるグループ
    group_ids: Vec<i32>,
    layout: String,
    is_pinned: bool,
    is_template: bool,
    /// データベースページ（layout = "database"）の列定義。壊れていたら null
    db_schema: Option<Value>,
    /// 行（データベースページの子ページ）の列の値。壊れていたら null
    properties: Option<Value>,
    /// タイムラインのページで、ゲストコメントを受け付けるか
    allow_guest_comments: bool,
    /// タイムラインのページで、リアクションを受け付けるか
    allow_reactions: bool,
    /// ロック（作った人と管理者以外は閲覧だけ）
    is_locked: bool,
}

pub async fn detail(db: &impl ConnectionTrait, p: page::Model) -> AppResult<PageDetail> {
    Ok(details(db, vec![p]).await?.pop().expect("1件渡したので1件返る"))
}

/// 複数のページをまとめて（ユーザー名・グループの取得を1回で済ませる）
pub async fn details(db: &impl ConnectionTrait, ps: Vec<page::Model>) -> AppResult<Vec<PageDetail>> {
    let names = user_names(db, ps.iter().flat_map(|p| [p.created_by, p.updated_by])).await?;
    let group_pages: Vec<i32> = ps.iter().filter(|p| p.visibility == access::GROUPS).map(|p| p.id).collect();
    let mut groups: HashMap<i32, Vec<i32>> = HashMap::new();
    if !group_pages.is_empty() {
        let rows: Vec<(i32, i32)> = page_group::Entity::find()
            .select_only()
            .columns([page_group::Column::PageId, page_group::Column::GroupId])
            .filter(page_group::Column::PageId.is_in(group_pages))
            .into_tuple()
            .all(db)
            .await?;
        for (p, g) in rows {
            groups.entry(p).or_default().push(g);
        }
        groups.values_mut().for_each(|v| v.sort_unstable());
    }
    Ok(ps
        .into_iter()
        .map(|p| {
            let g = groups.remove(&p.id).unwrap_or_default();
            detail_with(p, &names, g)
        })
        .collect())
}

fn detail_with(p: page::Model, names: &HashMap<i32, String>, group_ids: Vec<i32>) -> PageDetail {
    let name_of = |id: Option<i32>| id.and_then(|i| names.get(&i).cloned());
    PageDetail {
        id: p.id,
        parent_id: p.parent_id,
        title: p.title,
        icon: p.icon,
        // 壊れたJSONが入っていても閲覧画面は落とさない
        content: serde_json::from_str(&p.content).unwrap_or_else(|_| json!([])),
        created_by: name_of(p.created_by),
        created_by_id: p.created_by,
        updated_by: name_of(p.updated_by),
        created_at: p.created_at,
        updated_at: p.updated_at,
        version: p.version,
        is_draft: p.is_draft,
        visibility: p.visibility,
        group_ids,
        layout: p.layout,
        is_pinned: p.is_pinned,
        is_template: p.is_template,
        db_schema: p.db_schema.and_then(|s| serde_json::from_str(&s).ok()),
        properties: p.properties.and_then(|s| serde_json::from_str(&s).ok()),
        allow_guest_comments: p.allow_guest_comments,
        allow_reactions: p.allow_reactions,
        is_locked: p.is_locked,
    }
}

/// ロック中のページは、作った人と管理者しか編集できない（本文・タイトル・設定・版の復元）
pub fn ensure_editable(v: &Viewer, p: &page::Model) -> AppResult<()> {
    if p.is_locked && !v.can_manage(p.created_by) {
        return Err(AppError::BadRequest(
            "このページはロックされています。編集できるのは、ページを作った人と管理者だけです".into(),
        ));
    }
    Ok(())
}

/// ロック中のページは、作った人・管理者でも移動・ゴミ箱にできない（先にロックを外す）。
/// ゴミ箱は子孫もまとめて移すので、子孫にロック中のページがあっても断る
fn ensure_unlocked(all: &[PageSummary], ids: &[i32], root: i32, what: &str) -> AppResult<()> {
    if all.iter().any(|p| p.id == root && p.is_locked) {
        return Err(AppError::BadRequest(format!(
            "このページはロックされているため、{what}。先に「…」メニューからロックを外してください"
        )));
    }
    let ids: HashSet<i32> = ids.iter().copied().collect();
    let locked: Vec<String> = all
        .iter()
        .filter(|p| p.is_locked && p.id != root && ids.contains(&p.id))
        .map(|p| format!("「{}」", if p.title.trim().is_empty() { "無題" } else { p.title.as_str() }))
        .collect();
    if !locked.is_empty() {
        return Err(AppError::BadRequest(format!(
            "このページの下に、ロックされたページ{}があるため、{what}。ロックを外せるのは、そのページを作った人と管理者です",
            locked.join("")
        )));
    }
    Ok(())
}

/// ゴミ箱に入っていないページを取得（入っていれば 404）
pub async fn find_live_page(db: &impl ConnectionTrait, id: i32) -> AppResult<page::Model> {
    page::Entity::find_by_id(id)
        .filter(page::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

/// 更新用に行ロックを取って取得する（版番号の確認と加算の間に他の保存が割り込まないように）
pub async fn find_live_page_for_update(
    db: &impl ConnectionTrait,
    id: i32,
) -> AppResult<page::Model> {
    page::Entity::find_by_id(id)
        .filter(page::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn get_one(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    Path(id): Path<i32>,
) -> AppResult<Json<PageDetail>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let p = find_visible_page(&st.db, id, &v).await?;
    Ok(Json(detail(&st.db, p).await?))
}

fn validate_title(t: &str) -> AppResult<()> {
    if t.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::BadRequest("タイトルが長すぎます".into()));
    }
    Ok(())
}

fn validate_icon(t: &str) -> AppResult<()> {
    if t.chars().count() > MAX_ICON_CHARS {
        return Err(AppError::BadRequest("アイコンが長すぎます".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateReq {
    pub parent_id: Option<i32>,
    #[serde(default)]
    pub title: String,
    /// タイムラインへの投稿のように、本文付きで作るとき
    pub content: Option<Value>,
    /// 下書きとして作る（タイムラインの「下書き保存」）
    #[serde(default)]
    pub is_draft: bool,
    /// テンプレートから作るとき、テンプレートのアイコンを引き継ぐ
    pub icon: Option<String>,
    /// テンプレートから作るとき、テンプレートの表示形式を引き継ぐ
    pub layout: Option<String>,
    /// テンプレートから作るとき、テンプレートの列定義（データベースページ）を引き継ぐ
    pub db_schema: Option<Value>,
    /// テンプレートから作るとき true にする（本文があってもタイトルを自動生成しない）
    #[serde(default)]
    pub from_template: bool,
    /// 公開範囲（省略時は public。親の範囲は引き継がれるので、親と同じ範囲になる）
    pub visibility: Option<String>,
    /// 公開範囲が groups のとき、見られるグループ
    #[serde(default)]
    pub group_ids: Vec<i32>,
}

/// 本文の最初の行から、タイトルを作る（タイムラインの投稿用）
fn auto_title(content: &Value) -> String {
    let text = plaintext::extract(content);
    let line = text.lines().next().unwrap_or("").trim();
    let mut t: String = line.chars().take(AUTO_TITLE_CHARS).collect();
    if line.chars().count() > AUTO_TITLE_CHARS {
        t.push('…');
    }
    t
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<PageDetail>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let p = create_page(&st.db, &v, req).await?;
    Ok(Json(detail(&st.db, p).await?))
}

/// ページを兄弟の末尾に作る（API と CLI で共用）。親は viewer に見えるページでなければならない
pub async fn create_page(
    db: &DatabaseConnection,
    viewer: &Viewer,
    req: CreateReq,
) -> AppResult<page::Model> {
    let user_id = viewer.user_id().ok_or(AppError::Unauthorized)?;
    validate_title(&req.title)?;
    let visibility = req.visibility.unwrap_or_else(|| access::PUBLIC.into());
    let group_ids = access::validate(db, viewer, &visibility, &req.group_ids).await?;
    let content = match req.content {
        Some(c) => Some(sanitize::sanitize_blocks(c).map_err(|e| AppError::BadRequest(e.into()))?),
        None => None,
    };
    let title = match &content {
        Some(c) if req.title.trim().is_empty() && !req.from_template => auto_title(c),
        _ => req.title,
    };
    let icon = match &req.icon {
        Some(i) => {
            validate_icon(i)?;
            i.clone()
        }
        None => String::new(),
    };
    let layout = match req.layout {
        Some(l) if LAYOUTS.contains(&l.as_str()) => l,
        Some(_) => return Err(AppError::BadRequest("不明な表示形式です".into())),
        None => "page".into(),
    };
    if let Some(pid) = req.parent_id {
        find_visible_page(db, pid, viewer).await?;
    }
    // 兄弟の末尾に追加する
    let last = page::Entity::find()
        .filter(match req.parent_id {
            Some(pid) => page::Column::ParentId.eq(pid),
            None => page::Column::ParentId.is_null(),
        })
        .order_by_desc(page::Column::SortOrder)
        .one(db)
        .await?;
    let now = Utc::now();
    let txn = db.begin().await?;
    let p = page::ActiveModel {
        parent_id: Set(req.parent_id),
        title: Set(title),
        icon: Set(icon),
        content: Set(content.as_ref().map_or("[]".into(), Value::to_string)),
        search_text: Set(Some(content.as_ref().map(plaintext::extract).unwrap_or_default())),
        sort_order: Set(last.map_or(0, |l| l.sort_order + 1)),
        created_by: Set(Some(user_id)),
        updated_by: Set(Some(user_id)),
        created_at: Set(now),
        updated_at: Set(now),
        version: Set(1),
        is_draft: Set(req.is_draft),
        visibility: Set(visibility),
        layout: Set(layout),
        db_schema: Set(req.db_schema.map(|s| s.to_string())),
        is_pinned: Set(false),
        is_template: Set(false),
        ..Default::default()
    }
    .insert(&txn)
    .await?;
    access::set_page_groups(&txn, p.id, &group_ids).await?;
    if let Some(c) = &content {
        replace_links(&txn, p.id, c).await?;
        record_revision(&txn, &p, user_id, true).await?;
    }
    txn.commit().await?;
    Ok(p)
}

/// 保存後の状態を履歴に残す。同じ人が直近に始めた版があればそこへまとめる
pub async fn record_revision(
    db: &impl ConnectionTrait,
    p: &page::Model,
    user_id: i32,
    force_new: bool,
) -> AppResult<()> {
    let now = Utc::now();
    let latest = page_revision::Entity::find()
        .filter(page_revision::Column::PageId.eq(p.id))
        .order_by_desc(page_revision::Column::Id)
        .one(db)
        .await?;
    match latest {
        Some(r)
            if !force_new
                && r.created_by == Some(user_id)
                && now - r.started_at < Duration::minutes(REVISION_MERGE_WINDOW_MIN) =>
        {
            let mut am: page_revision::ActiveModel = r.into();
            am.title = Set(p.title.clone());
            am.icon = Set(p.icon.clone());
            am.content = Set(p.content.clone());
            am.updated_at = Set(now);
            am.update(db).await?;
        }
        _ => {
            page_revision::ActiveModel {
                page_id: Set(p.id),
                title: Set(p.title.clone()),
                icon: Set(p.icon.clone()),
                content: Set(p.content.clone()),
                created_by: Set(Some(user_id)),
                started_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(db)
            .await?;
        }
    }
    Ok(())
}

pub struct PagePatch {
    pub title: Option<String>,
    pub icon: Option<String>,
    pub content: Option<Value>,
}

/// タイトル・アイコン・本文を更新し、履歴を残す（通常の保存と版の復元で共用）
pub async fn apply_patch(
    db: &impl ConnectionTrait,
    p: page::Model,
    patch: PagePatch,
    user_id: i32,
    force_new_revision: bool,
) -> AppResult<page::Model> {
    let id = p.id;
    let version = p.version;
    let mut am: page::ActiveModel = p.into();
    if let Some(t) = patch.title {
        validate_title(&t)?;
        am.title = Set(t);
    }
    if let Some(i) = patch.icon {
        validate_icon(&i)?;
        am.icon = Set(i);
    }
    if let Some(c) = patch.content {
        let c = sanitize::sanitize_blocks(c).map_err(|e| AppError::BadRequest(e.into()))?;
        am.search_text = Set(Some(plaintext::extract(&c)));
        replace_links(db, id, &c).await?;
        am.content = Set(c.to_string());
    }
    am.version = Set(version + 1);
    am.updated_by = Set(Some(user_id));
    am.updated_at = Set(Utc::now());
    let p = am.update(db).await?;
    record_revision(db, &p, user_id, force_new_revision).await?;
    Ok(p)
}

/// このページから出ているリンクを、本文の内容で作り直す
pub(crate) async fn replace_links(db: &impl ConnectionTrait, from: i32, content: &Value) -> AppResult<()> {
    page_link::Entity::delete_many()
        .filter(page_link::Column::FromPageId.eq(from))
        .exec(db)
        .await?;
    let links: Vec<page_link::ActiveModel> = refs::page_links(content)
        .into_iter()
        .filter(|&to| to != from)
        .map(|to| page_link::ActiveModel {
            from_page_id: Set(from),
            to_page_id: Set(to),
        })
        .collect();
    if !links.is_empty() {
        page_link::Entity::insert_many(links).exec(db).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct UpdateReq {
    /// 編集を始めたときの版番号。今の版と違えば 409 を返す（省略時は確認せず上書き）
    base_version: Option<i32>,
    title: Option<String>,
    icon: Option<String>,
    content: Option<Value>,
}

pub async fn update(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<PageDetail>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let txn = st.db.begin().await?;
    let p = find_live_page_for_update(&txn, id).await?;
    ensure_visible(&txn, id, &v).await?;
    ensure_editable(&v, &p)?;
    if req.base_version.is_some_and(|v| v != p.version) {
        let latest = serde_json::to_value(detail(&txn, p).await?).map_err(anyhow::Error::from)?;
        return Err(AppError::Conflict(latest));
    }
    let patch = PagePatch {
        title: req.title,
        icon: req.icon,
        content: req.content,
    };
    let p = apply_patch(&txn, p, patch, cu.user.id, false).await?;
    txn.commit().await?;
    Ok(Json(detail(&st.db, p).await?))
}

/// このページへリンクしているページ（ゴミ箱のものは除く）
pub async fn backlinks(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Vec<PageSummary>>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    find_visible_page(&st.db, id, &v).await?;
    let from: Vec<i32> = page_link::Entity::find()
        .select_only()
        .column(page_link::Column::FromPageId)
        .filter(page_link::Column::ToPageId.eq(id))
        .into_tuple()
        .all(&st.db)
        .await?;
    if from.is_empty() {
        return Ok(Json(vec![]));
    }
    Ok(Json(
        visible_summaries(&st.db, &v)
            .await?
            .into_iter()
            .filter(|p| from.contains(&p.id))
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct SettingsReq {
    /// 下書き・公開範囲は、作った人と管理者だけが変えられる
    is_draft: Option<bool>,
    visibility: Option<String>,
    /// visibility と一緒に送る（groups のときの見られるグループ）
    group_ids: Option<Vec<i32>>,
    layout: Option<String>,
    is_pinned: Option<bool>,
    is_template: Option<bool>,
    /// データベースページ自身の列定義（置き換え）。[] で全削除
    db_schema: Option<Value>,
    /// 行の列の値（置き換え）。{} で全消去
    properties: Option<Value>,
    /// タイムラインのページでのゲストコメント・リアクションの受け付け
    allow_guest_comments: Option<bool>,
    allow_reactions: Option<bool>,
    /// ロックは、作った人と管理者だけが掛け外しできる
    is_locked: Option<bool>,
}

/// データベースページの列定義から、kind = "url" の列 id を集める
fn url_column_ids(db_schema: &Value) -> HashSet<String> {
    db_schema
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c.get("kind").and_then(Value::as_str) == Some("url"))
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// どの列が url 型か分からないとき、どれが不正かも判断できないので全ての文字列値を保守的に空文字にする
fn blank_strings(obj: &mut serde_json::Map<String, Value>) {
    for v in obj.values_mut() {
        if v.is_string() {
            *v = json!("");
        }
    }
}

/// url 型の列の値を、許可したスキームだけを通すように検査する（不正なら空文字に）。
/// row の親は必ず database ページのはずだが、その不変条件が崩れていて列定義が分からない場合は
/// fail-closed にし、どの列が url 型か判断できない以上すべての文字列値を空文字にする
async fn sanitize_properties(
    db: &impl ConnectionTrait,
    parent_id: Option<i32>,
    mut properties: Value,
) -> AppResult<Value> {
    let Some(obj) = properties.as_object_mut() else {
        return Ok(properties);
    };
    let schema = 'schema: {
        let Some(parent_id) = parent_id else { break 'schema None };
        let Some(parent) = page::Entity::find_by_id(parent_id).one(db).await? else { break 'schema None };
        parent.db_schema.and_then(|s| serde_json::from_str::<Value>(&s).ok())
    };
    let Some(schema) = schema else {
        blank_strings(obj);
        return Ok(properties);
    };
    let url_ids = url_column_ids(&schema);
    for id in url_ids {
        if let Some(v) = obj.get_mut(&id) {
            if let Some(s) = v.as_str() {
                if !sanitize::is_safe_url(s) {
                    *v = json!("");
                }
            }
        }
    }
    Ok(properties)
}

/// 公開 / 下書き、表示形式、ピン留め、テンプレート、データベースの列定義・行の値、
/// ゲストコメント・リアクションの受け付けの切り替え（本文の版は増やさない）
pub async fn update_settings(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<SettingsReq>,
) -> AppResult<Json<PageDetail>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let p = find_visible_page(&st.db, id, &v).await?;
    let parent_id = p.parent_id;
    let changes_access = req.is_draft.is_some() || req.visibility.is_some() || req.group_ids.is_some();
    if changes_access && !v.can_manage(p.created_by) {
        return Err(AppError::BadRequest(
            "下書き・公開範囲を変えられるのは、ページを作った人と管理者だけです".into(),
        ));
    }
    if req.is_locked.is_some() && !v.can_manage(p.created_by) {
        return Err(AppError::BadRequest(
            "ロックを掛けたり外したりできるのは、ページを作った人と管理者だけです".into(),
        ));
    }
    ensure_editable(&v, &p)?;
    // 公開範囲とグループは組で置き換える（片方だけなら、もう片方は今の値）
    let new_access = match (&req.visibility, &req.group_ids) {
        (None, None) => None,
        (vis, gs) => {
            let vis = vis.clone().unwrap_or_else(|| p.visibility.clone());
            let gs = match gs {
                Some(g) => g.clone(),
                None => access::groups_of_page(&st.db, id).await?,
            };
            Some((access::validate(&st.db, &v, &vis, &gs).await?, vis))
        }
    };
    // 固定できるのはタイムラインの直下の投稿だけ（返信は固定しない）
    if req.is_pinned == Some(true) {
        let parent = match parent_id {
            Some(pid) => page::Entity::find_by_id(pid).one(&st.db).await?,
            None => None,
        };
        if parent.is_none_or(|pp| pp.layout != "timeline") {
            return Err(AppError::BadRequest("固定できるのはタイムラインの投稿だけです".into()));
        }
    }
    let mut am: page::ActiveModel = p.into();
    if let Some(d) = req.is_draft {
        am.is_draft = Set(d);
    }
    if let Some(pin) = req.is_pinned {
        am.is_pinned = Set(pin);
    }
    if let Some(t) = req.is_template {
        am.is_template = Set(t);
    }
    if let Some(l) = req.layout {
        if !LAYOUTS.contains(&l.as_str()) {
            return Err(AppError::BadRequest("不明な表示形式です".into()));
        }
        am.layout = Set(l);
    }
    if let Some(v) = req.allow_guest_comments {
        am.allow_guest_comments = Set(v);
    }
    if let Some(v) = req.allow_reactions {
        am.allow_reactions = Set(v);
    }
    if let Some(l) = req.is_locked {
        am.is_locked = Set(l);
    }
    if let Some(s) = req.db_schema {
        am.db_schema = Set(Some(s.to_string()));
    }
    if let Some(props) = req.properties {
        let props = sanitize_properties(&st.db, parent_id, props).await?;
        am.properties = Set(Some(props.to_string()));
    }
    let txn = st.db.begin().await?;
    if let Some((groups, vis)) = new_access {
        am.visibility = Set(vis);
        access::set_page_groups(&txn, id, &groups).await?;
    }
    let p = am.update(&txn).await?;
    txn.commit().await?;
    Ok(Json(detail(&st.db, p).await?))
}

#[derive(Serialize, FromQueryResult)]
pub struct RecentItem {
    id: i32,
    title: String,
    icon: String,
    is_draft: bool,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    updated_by: Option<i32>,
    #[serde(skip)]
    search_text: Option<String>,
}

/// 最近更新されたページ（タイムラインの投稿は除く）
pub async fn recent(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
) -> AppResult<Json<Vec<Value>>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let hidden = excluded_from_recent(&st.db, &v).await?;
    let mut q = page::Entity::find()
        .select_only()
        .columns([
            page::Column::Id,
            page::Column::Title,
            page::Column::Icon,
            page::Column::IsDraft,
            page::Column::UpdatedAt,
            page::Column::UpdatedBy,
            page::Column::SearchText,
        ])
        .filter(page::Column::DeletedAt.is_null());
    if !hidden.is_empty() {
        q = q.filter(page::Column::Id.is_not_in(hidden));
    }
    let rows = q
        .order_by_desc(page::Column::UpdatedAt)
        .limit(RECENT_LIMIT)
        .into_model::<RecentItem>()
        .all(&st.db)
        .await?;
    let names = user_names(&st.db, rows.iter().map(|r| r.updated_by)).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| {
                let mut v = serde_json::to_value(&r).unwrap_or_default();
                v["updated_by"] = json!(r.updated_by.and_then(|i| names.get(&i).cloned()));
                v["snippet"] = json!(snippet(r.search_text.as_deref().unwrap_or("")));
                v
            })
            .collect(),
    ))
}

/// 本文の冒頭（一覧・フィード・OGP の説明文用）
pub fn snippet(text: &str) -> String {
    const N: usize = 120;
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s: String = flat.chars().take(N).collect();
    if flat.chars().count() > N {
        s.push('…');
    }
    s
}

#[derive(Deserialize)]
pub struct PostsQuery {
    /// この id より古い投稿を返す（続きの読み込み）
    before: Option<i32>,
    /// 月別アーカイブ（YYYY-MM）。指定するとその月の投稿だけを、ピン留めも区別せずに返す
    month: Option<String>,
    /// 月の区切りに使う、閲覧している人のタイムゾーン（UTC から東へ何分。日本なら 540）
    #[serde(default)]
    tz: i64,
}

fn clamp_tz(tz: i64) -> i64 {
    tz.clamp(-14 * 60, 14 * 60)
}

/// YYYY-MM の月の始まりと次の月の始まり（tz の 0時）を UTC で
fn month_range(month: &str, tz: i64) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").ok()?;
    let next = start.checked_add_months(chrono::Months::new(1))?;
    let utc = |d: NaiveDate| d.and_hms_opt(0, 0, 0).unwrap().and_utc() - Duration::minutes(tz);
    Some((utc(start), utc(next)))
}

/// タイムラインの投稿（子ページ）を絞り込むクエリ。閲覧者には下書きを含めない
async fn visible_children(
    db: &DatabaseConnection,
    id: i32,
    viewer: &Viewer,
) -> AppResult<sea_orm::Select<page::Entity>> {
    find_visible_page(db, id, viewer).await?;
    let hidden = hidden_for(db, viewer).await?;
    let q = page::Entity::find()
        .filter(page::Column::ParentId.eq(id))
        .filter(page::Column::DeletedAt.is_null());
    Ok(if hidden.is_empty() {
        q
    } else {
        q.filter(page::Column::Id.is_not_in(hidden))
    })
}

#[derive(FromQueryResult)]
struct RowRecord {
    id: i32,
    parent_id: Option<i32>,
    title: String,
    icon: String,
    sort_order: i32,
    is_draft: bool,
    properties: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by: Option<i32>,
    is_locked: bool,
}

/// データベースページの行（子ページ）を sort_order 順に、列の値付きで返す
pub async fn rows(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Vec<Value>>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let records = visible_children(&st.db, id, &v)
        .await?
        .select_only()
        .columns([
            page::Column::Id,
            page::Column::ParentId,
            page::Column::Title,
            page::Column::Icon,
            page::Column::SortOrder,
            page::Column::IsDraft,
            page::Column::Properties,
            page::Column::CreatedAt,
            page::Column::UpdatedAt,
            page::Column::CreatedBy,
            page::Column::IsLocked,
        ])
        .order_by_asc(page::Column::SortOrder)
        .order_by_asc(page::Column::Id)
        .into_model::<RowRecord>()
        .all(&st.db)
        .await?;
    Ok(Json(
        records
            .into_iter()
            .map(|r| {
                let properties = r
                    .properties
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or_else(|| json!({}));
                json!({
                    "id": r.id,
                    "parent_id": r.parent_id,
                    "title": r.title,
                    "icon": r.icon,
                    "sort_order": r.sort_order,
                    "is_draft": r.is_draft,
                    "properties": properties,
                    "created_at": r.created_at,
                    "updated_at": r.updated_at,
                    "is_locked": r.is_locked,
                    // ロック中の行は、作った人と管理者だけが編集できる
                    "can_edit": !r.is_locked || v.can_manage(r.created_by),
                })
            })
            .collect(),
    ))
}

/// タイムライン：子ページを新しい順に、本文付きで返す。
/// 最初の読み込み（before なし）では、ピン留めした投稿を pinned として別に返す（posts には含めない）
pub async fn posts(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    jar: CookieJar,
    Path(id): Path<i32>,
    Query(q): Query<PostsQuery>,
) -> AppResult<Json<Value>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let children = visible_children(&st.db, id, &v).await?;
    let timeline = find_live_page(&st.db, id).await?;
    let range = match q.month.as_deref() {
        Some(m) => Some(
            month_range(m, clamp_tz(q.tz))
                .ok_or_else(|| AppError::BadRequest("月の指定が正しくありません".into()))?,
        ),
        None => None,
    };
    let pinned = match (q.before, range) {
        (None, None) => {
            children
                .clone()
                .filter(page::Column::IsPinned.eq(true))
                .order_by_desc(page::Column::Id)
                .all(&st.db)
                .await?
        }
        _ => vec![],
    };
    let mut query = match range {
        Some((from, to)) => children
            .filter(page::Column::CreatedAt.gte(from))
            .filter(page::Column::CreatedAt.lt(to)),
        None => children.filter(page::Column::IsPinned.eq(false)),
    };
    if let Some(b) = q.before {
        query = query.filter(page::Column::Id.lt(b));
    }
    let mut rows = query
        .order_by_desc(page::Column::Id)
        .limit(MAX_POSTS_PER_REQUEST + 1)
        .all(&st.db)
        .await?;
    let has_more = rows.len() as u64 > MAX_POSTS_PER_REQUEST;
    rows.truncate(MAX_POSTS_PER_REQUEST as usize);
    let out = with_replies(&st.db, &v, rows).await?;
    let pinned_out = with_replies(&st.db, &v, pinned).await?;
    let top: HashSet<i32> = out.iter().chain(&pinned_out).map(|n| n.post.id).collect();
    let mut ids = Vec::new();
    collect_ids(&out, &mut ids);
    collect_ids(&pinned_out, &mut ids);
    let policy = interactions::policy_of(&st, &timeline);
    let guest_hidden = guest_hidden(&st.db, &v).await?;
    let inter = interactions::collect(&st.db, &jar, policy, &ids, &top, &guest_hidden).await?;
    Ok(Json(json!({
        "pinned": pinned_out,
        "posts": out,
        "has_more": has_more,
        "interactions": inter,
    })))
}

/// 木の中の投稿・返信の id を全部
fn collect_ids(nodes: &[PostNode], out: &mut Vec<i32>) {
    for n in nodes {
        out.push(n.post.id);
        collect_ids(&n.replies, out);
    }
}

/// タイムラインの投稿・返信と、その下の返信の木
#[derive(Serialize)]
pub struct PostNode {
    #[serde(flatten)]
    post: PageDetail,
    replies: Vec<PostNode>,
}

/// roots それぞれに、その下の返信（見てよいものだけ）を新しい順の木にして付ける
async fn with_replies(
    db: &impl ConnectionTrait,
    viewer: &Viewer,
    roots: Vec<page::Model>,
) -> AppResult<Vec<PostNode>> {
    if roots.is_empty() {
        return Ok(vec![]);
    }
    let all = all_summaries(db).await?;
    let hidden = hidden_in(db, &all, viewer).await?;
    let pairs: Vec<(i32, Option<i32>)> = all
        .iter()
        .filter(|p| !hidden.contains(&p.id))
        .map(|p| (p.id, p.parent_id))
        .collect();
    let reply_ids: Vec<i32> = roots
        .iter()
        .flat_map(|r| subtree_levels(&pairs, r.id).into_iter().skip(1).flatten())
        .collect();
    let replies = if reply_ids.is_empty() {
        vec![]
    } else {
        page::Entity::find()
            .filter(page::Column::Id.is_in(reply_ids))
            .filter(page::Column::DeletedAt.is_null())
            .order_by_desc(page::Column::Id)
            .all(db)
            .await?
    };
    let mut kids: HashMap<i32, Vec<PageDetail>> = HashMap::new();
    for d in details(db, replies).await? {
        if let Some(pid) = d.parent_id {
            kids.entry(pid).or_default().push(d);
        }
    }
    fn build(post: PageDetail, kids: &mut HashMap<i32, Vec<PageDetail>>) -> PostNode {
        let replies = kids
            .remove(&post.id)
            .unwrap_or_default()
            .into_iter()
            .map(|d| build(d, kids))
            .collect();
        PostNode { post, replies }
    }
    Ok(details(db, roots)
        .await?
        .into_iter()
        .map(|d| build(d, &mut kids))
        .collect())
}

/// タイムラインの投稿・返信のスレッド：所属するタイムライン、元の投稿から親までの投稿（古い順）、
/// このページへの返信の木。タイムラインの配下でなければ 404
/// ログインしていない人に見えないページ（ゲストコメントを受け付けない）。ログインしていなければ、
/// そもそも見えないページは応答に入らないので空でよい
async fn guest_hidden(db: &impl ConnectionTrait, viewer: &Viewer) -> AppResult<HashSet<i32>> {
    if viewer.is_guest() { Ok(HashSet::new()) } else { hidden_for(db, &Viewer::Guest).await }
}

pub async fn thread(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    jar: CookieJar,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let p = find_visible_page(&st.db, id, &v).await?;
    let all = visible_summaries(&st.db, &v).await?;
    let (timeline_id, chain) = thread_position(&all, id).ok_or(AppError::NotFound)?;
    let timeline = all.iter().find(|s| s.id == timeline_id).cloned();
    let mut ancestors = if chain.is_empty() {
        vec![]
    } else {
        page::Entity::find()
            .filter(page::Column::Id.is_in(chain.clone()))
            .all(&st.db)
            .await?
    };
    // chain は近い順なので、元の投稿が先頭に来るよう逆に並べる
    ancestors.sort_by_key(|a| std::cmp::Reverse(chain.iter().position(|&c| c == a.id)));
    let replies = with_replies(&st.db, &v, vec![p])
        .await?
        .pop()
        .map_or_else(Vec::new, |n| n.replies);
    // このページと返信の件数（コメントは、このページがタイムラインの直下の投稿のときだけ）
    let mut ids = vec![id];
    collect_ids(&replies, &mut ids);
    let top: HashSet<i32> = if chain.is_empty() { HashSet::from([id]) } else { HashSet::new() };
    let policy = match page::Entity::find_by_id(timeline_id).one(&st.db).await? {
        Some(t) => interactions::policy_of(&st, &t),
        None => interactions::Policy::default(),
    };
    let guest_hidden = guest_hidden(&st.db, &v).await?;
    let inter = interactions::collect(&st.db, &jar, policy, &ids, &top, &guest_hidden).await?;
    Ok(Json(json!({
        "timeline": timeline,
        "ancestors": details(&st.db, ancestors).await?,
        "replies": replies,
        "interactions": inter,
    })))
}

#[derive(Deserialize)]
pub struct ArchiveQuery {
    #[serde(default)]
    tz: i64,
}

/// タイムラインの月別アーカイブ：投稿のある月と件数を新しい順に
pub async fn archive(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    Path(id): Path<i32>,
    Query(q): Query<ArchiveQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let tz = clamp_tz(q.tz);
    let v = access::viewer(&st.db, cu.as_ref()).await?;
    let created: Vec<DateTime<Utc>> = visible_children(&st.db, id, &v)
        .await?
        .select_only()
        .column(page::Column::CreatedAt)
        .into_tuple()
        .all(&st.db)
        .await?;
    Ok(Json(
        count_by_month(&created, tz)
            .into_iter()
            .map(|(month, count)| json!({ "month": month, "count": count }))
            .collect(),
    ))
}

/// 日時を tz の暦で月ごとに数える（新しい月から）
fn count_by_month(times: &[DateTime<Utc>], tz: i64) -> Vec<(String, usize)> {
    let mut counts: HashMap<(i32, u32), usize> = HashMap::new();
    for t in times {
        let local = *t + Duration::minutes(tz);
        *counts.entry((local.year(), local.month())).or_default() += 1;
    }
    let mut out: Vec<_> = counts.into_iter().collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.0));
    out.into_iter()
        .map(|((y, m), n)| (format!("{y:04}-{m:02}"), n))
        .collect()
}

#[derive(Deserialize)]
pub struct MoveReq {
    parent_id: Option<i32>,
    /// 新しい親の子の中での位置（0始まり）。省略時は末尾
    index: Option<usize>,
}

pub async fn move_page(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<MoveReq>,
) -> AppResult<Json<Vec<PageSummary>>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let all = all_summaries(&st.db).await?;
    let hidden = hidden_in(&st.db, &all, &v).await?;
    let Some(me) = all.iter().find(|p| p.id == id) else { return Err(AppError::NotFound) };
    if hidden.contains(&id) || req.parent_id.is_some_and(|pid| hidden.contains(&pid)) {
        return Err(AppError::NotFound);
    }
    // 祖先の下書き・公開範囲の制限が変わる移動は、見える人が増える（外へ出す）か、減る（他の人の
    // ページを自分の下書きの下へ入れると、作った人も見られなくなる）。それを決められるのは作った人と管理者。
    // 同じ制限の中での付け替え（同じグループのページの下どうし）は誰でもできる
    if access::restricting_ancestors(&all, me.parent_id)
        != access::restricting_ancestors(&all, req.parent_id)
        && !v.can_manage(me.created_by)
    {
        return Err(AppError::BadRequest(
            "移す前と後で、親ページの下書き・公開範囲の制限が変わります。そういう移動ができるのは、このページを作った人と管理者だけです（見られる人が変わるため）".into(),
        ));
    }
    move_to(&st.db, id, req.parent_id, req.index).await?;
    Ok(Json(visible_summaries(&st.db, &v).await?))
}

/// ページを parent_id の子の index 番目（省略時は末尾）へ移す（API と CLI で共用）
pub async fn move_to(
    db: &DatabaseConnection,
    id: i32,
    parent_id: Option<i32>,
    index: Option<usize>,
) -> AppResult<()> {
    let all = all_summaries(db).await?;
    if !all.iter().any(|p| p.id == id) {
        return Err(AppError::NotFound);
    }
    // 子ページは一緒に動くだけで、親との関係は変わらないので、子孫のロックは見ない
    ensure_unlocked(&all, &[id], id, "移動できません")?;
    // 自分自身や自分の子孫の下には移動できない（ゴミ箱内のページの下にも移動できない）
    if let Some(pid) = parent_id {
        let parent_of: HashMap<i32, Option<i32>> =
            all.iter().map(|p| (p.id, p.parent_id)).collect();
        let mut cur = Some(pid);
        while let Some(c) = cur {
            if c == id {
                return Err(AppError::BadRequest(
                    "自分自身やその子ページの下には移動できません".into(),
                ));
            }
            cur = *parent_of.get(&c).ok_or(AppError::NotFound)?;
        }
    }

    let mut siblings: Vec<i32> = all
        .iter()
        .filter(|p| p.parent_id == parent_id && p.id != id)
        .map(|p| p.id)
        .collect();
    let idx = index.unwrap_or(siblings.len()).min(siblings.len());
    siblings.insert(idx, id);

    let txn = db.begin().await?;
    for (order, sid) in siblings.into_iter().enumerate() {
        page::ActiveModel {
            id: Set(sid),
            parent_id: Set(parent_id),
            sort_order: Set(order as i32),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok(())
}

/// ゴミ箱へ移す（子孫もまとめて）。完全な削除は trash.rs
pub async fn delete(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let all = all_summaries(&st.db).await?;
    let hidden = hidden_in(&st.db, &all, &v).await?;
    if hidden.contains(&id) {
        return Err(AppError::NotFound);
    }
    // 見えないページ（他の人の下書きなど）まで一緒にゴミ箱へ入れてしまわないように
    let pairs: Vec<(i32, Option<i32>)> = all.iter().map(|p| (p.id, p.parent_id)).collect();
    if subtree_levels(&pairs, id).into_iter().flatten().any(|i| hidden.contains(&i)) {
        return Err(AppError::BadRequest(
            "このページの下に、あなたには見えないページ（他の人の下書きなど）があるため、ゴミ箱へ移せません".into(),
        ));
    }
    let count = trash_page(&st.db, id, cu.user.id).await?;
    Ok(Json(json!({ "trashed": count })))
}

/// ページを子孫ごとゴミ箱へ移し、移した件数を返す（API と CLI で共用）
pub async fn trash_page(db: &DatabaseConnection, id: i32, user_id: i32) -> AppResult<usize> {
    let all = all_summaries(db).await?;
    if !all.iter().any(|p| p.id == id) {
        return Err(AppError::NotFound);
    }
    // 既にゴミ箱にある子孫は all に含まれないので、それぞれの削除単位のまま残る
    let pairs: Vec<(i32, Option<i32>)> = all.iter().map(|p| (p.id, p.parent_id)).collect();
    let ids: Vec<i32> = subtree_levels(&pairs, id).into_iter().flatten().collect();
    ensure_unlocked(&all, &ids, id, "ゴミ箱へ移せません")?;
    let count = ids.len();
    page::Entity::update_many()
        .col_expr(page::Column::DeletedAt, Utc::now().into())
        .col_expr(page::Column::DeletedBy, user_id.into())
        .col_expr(page::Column::TrashRootId, id.into())
        .filter(page::Column::Id.is_in(ids))
        .exec(db)
        .await?;
    Ok(count)
}

/// 検索用テキストが未生成のページ（マイグレーション直後のデータ）について、検索用テキストとリンク表を作る
pub async fn backfill_index(db: &DatabaseConnection) -> AppResult<()> {
    let pages = page::Entity::find()
        .filter(page::Column::SearchText.is_null())
        .all(db)
        .await?;
    for p in pages {
        let v: Value = serde_json::from_str(&p.content).unwrap_or_else(|_| json!([]));
        replace_links(db, p.id, &v).await?;
        let mut am: page::ActiveModel = p.into();
        am.search_text = Set(Some(plaintext::extract(&v)));
        am.update(db).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn summary(id: i32, parent_id: Option<i32>, layout: &str) -> PageSummary {
        PageSummary {
            id,
            parent_id,
            title: String::new(),
            icon: String::new(),
            sort_order: 0,
            is_draft: false,
            visibility: "public".into(),
            created_by: None,
            layout: layout.into(),
            is_template: false,
            is_locked: false,
        }
    }

    #[test]
    fn replies_belong_to_the_timeline_thread() {
        // 1 タイムライン → 2 投稿 → 3 返信 → 4 返信、5 は普通のページの子
        let all = vec![
            summary(1, None, "timeline"),
            summary(2, Some(1), "page"),
            summary(3, Some(2), "page"),
            summary(4, Some(3), "page"),
            summary(6, None, "page"),
            summary(5, Some(6), "page"),
        ];
        assert_eq!(timeline_post_ids(&all), HashSet::from([2, 3, 4]));
        assert_eq!(thread_position(&all, 2), Some((1, vec![])));
        assert_eq!(thread_position(&all, 4), Some((1, vec![3, 2])));
        assert_eq!(thread_position(&all, 1), None);
        assert_eq!(thread_position(&all, 5), None);
    }

    #[test]
    fn locked_pages_block_move_and_trash() {
        // 1 → 2（ロック）→ 3、1 → 4
        let mut all = vec![
            summary(1, None, "page"),
            summary(2, Some(1), "page"),
            summary(3, Some(2), "page"),
            summary(4, Some(1), "page"),
        ];
        all[1].is_locked = true;
        all[1].title = "鍵".into();
        // ロック中のページ自身は、移動もゴミ箱もできない
        assert!(ensure_unlocked(&all, &[2], 2, "移動できません").is_err());
        assert!(ensure_unlocked(&all, &[2, 3], 2, "ゴミ箱へ移せません").is_err());
        // 子孫にロック中のページがあれば、親もゴミ箱にできない（名前を出す）
        match ensure_unlocked(&all, &[1, 2, 3, 4], 1, "ゴミ箱へ移せません") {
            Err(AppError::BadRequest(m)) => assert!(m.contains("「鍵」"), "{m}"),
            _ => panic!("断るはず"),
        }
        // ロックは子には効かない
        assert!(ensure_unlocked(&all, &[3], 3, "ゴミ箱へ移せません").is_ok());
        assert!(ensure_unlocked(&all, &[1], 1, "移動できません").is_ok());
        assert!(ensure_unlocked(&all, &[4], 4, "ゴミ箱へ移せません").is_ok());
    }

    #[test]
    fn locked_pages_are_edited_only_by_owner_and_admin() {
        let now = Utc::now();
        let mut p = page::Model {
            id: 1,
            parent_id: None,
            title: String::new(),
            icon: String::new(),
            content: "[]".into(),
            sort_order: 0,
            created_by: Some(10),
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
            layout: "page".into(),
            is_pinned: false,
            db_schema: None,
            properties: None,
            is_template: false,
            allow_guest_comments: false,
            allow_reactions: false,
            is_locked: false,
        };
        let owner = Viewer::User { id: 10, is_admin: false, groups: HashSet::new() };
        let other = Viewer::User { id: 11, is_admin: false, groups: HashSet::new() };
        let admin = Viewer::User { id: 12, is_admin: true, groups: HashSet::new() };
        assert!(ensure_editable(&other, &p).is_ok());
        p.is_locked = true;
        assert!(ensure_editable(&owner, &p).is_ok());
        assert!(ensure_editable(&admin, &p).is_ok());
        assert!(ensure_editable(&other, &p).is_err());
    }

    #[test]
    fn month_range_uses_viewer_timezone() {
        // 日本時間の 2026年10月 = UTC の 9/30 15:00 〜 10/31 15:00
        let (from, to) = month_range("2026-10", 540).unwrap();
        assert_eq!(from, utc("2026-09-30T15:00:00Z"));
        assert_eq!(to, utc("2026-10-31T15:00:00Z"));
        let (from, to) = month_range("2026-12", 0).unwrap();
        assert_eq!(from, utc("2026-12-01T00:00:00Z"));
        assert_eq!(to, utc("2027-01-01T00:00:00Z"));
        assert!(month_range("2026-13", 0).is_none());
        assert!(month_range("2026-1x", 0).is_none());
        assert!(month_range("", 0).is_none());
    }

    #[test]
    fn counts_posts_by_local_month() {
        let times = [
            utc("2026-09-30T14:59:00Z"), // 日本時間 9/30 23:59
            utc("2026-09-30T15:00:00Z"), // 日本時間 10/1 0:00
            utc("2026-10-20T00:00:00Z"),
            utc("2025-12-31T23:00:00Z"), // 日本時間 2026/1/1
        ];
        assert_eq!(
            count_by_month(&times, 540),
            vec![
                ("2026-10".to_string(), 2),
                ("2026-09".to_string(), 1),
                ("2026-01".to_string(), 1),
            ]
        );
        assert_eq!(count_by_month(&times, 0)[2], ("2025-12".to_string(), 1));
    }

    #[test]
    fn blank_strings_clears_strings_but_keeps_other_types() {
        let mut v = json!({"a": "keep-out", "b": 1, "c": null, "d": true});
        let Value::Object(obj) = &mut v else { unreachable!() };
        blank_strings(obj);
        assert_eq!(v, json!({"a": "", "b": 1, "c": null, "d": true}));
    }
}
