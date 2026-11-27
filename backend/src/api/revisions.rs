use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait};
use serde::Serialize;
use serde_json::{Value, json};

use super::pages::{self, PageDetail, PagePatch};
use crate::{
    AppState,
    access,
    auth::AuthUser,
    entity::page_revision,
    error::{AppError, AppResult},
};

const LIST_LIMIT: u64 = 200;

#[derive(Serialize)]
pub struct RevisionSummary {
    id: i32,
    title: String,
    icon: String,
    created_by: Option<String>,
    started_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// 履歴は編集者のみ閲覧可（削除した内容が公開されないように）
pub async fn list(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(page_id): Path<i32>,
) -> AppResult<Json<Vec<RevisionSummary>>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    pages::find_visible_page(&st.db, page_id, &v).await?;
    let revs = page_revision::Entity::find()
        .filter(page_revision::Column::PageId.eq(page_id))
        .order_by_desc(page_revision::Column::Id)
        .limit(LIST_LIMIT)
        .all(&st.db)
        .await?;
    let names = pages::user_names(&st.db, revs.iter().map(|r| r.created_by)).await?;
    Ok(Json(
        revs.into_iter()
            .map(|r| RevisionSummary {
                id: r.id,
                created_by: r.created_by.and_then(|i| names.get(&i).cloned()),
                title: r.title,
                icon: r.icon,
                started_at: r.started_at,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

async fn find_revision(st: &AppState, id: i32) -> AppResult<page_revision::Model> {
    page_revision::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn get_one(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let r = find_revision(&st, id).await?;
    let v = access::viewer(&st.db, Some(&cu)).await?;
    pages::ensure_visible_incl_trash(&st.db, r.page_id, &v).await?;
    Ok(Json(json!({
        "id": r.id,
        "page_id": r.page_id,
        "title": r.title,
        "icon": r.icon,
        "content": serde_json::from_str::<Value>(&r.content).unwrap_or_else(|_| json!([])),
        "started_at": r.started_at,
        "updated_at": r.updated_at,
    })))
}

/// 指定した版の内容でページを上書きする。復元自体も新しい版として残る
pub async fn restore(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
) -> AppResult<Json<PageDetail>> {
    let r = find_revision(&st, id).await?;
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let txn = st.db.begin().await?;
    let p = pages::find_live_page_for_update(&txn, r.page_id).await?;
    pages::ensure_visible(&txn, r.page_id, &v).await?;
    pages::ensure_editable(&v, &p)?;
    let patch = PagePatch {
        title: Some(r.title),
        icon: Some(r.icon),
        content: Some(serde_json::from_str(&r.content).unwrap_or_else(|_| json!([]))),
    };
    // 直前までの編集が同じ版にまとめられて消えないよう、必ず新しい版にする
    let p = pages::apply_patch(&txn, p, patch, cu.user.id, true).await?;
    txn.commit().await?;
    Ok(Json(pages::detail(&st.db, p).await?))
}
