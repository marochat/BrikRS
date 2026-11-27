//! ブロックスニペット：「/」メニューから挿入する、よく使う複数ブロックの組み合わせ。
//! ログインしている編集者なら誰でも作成・編集・削除できる（身内規模の運用が前提）

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppState,
    auth::AuthUser,
    entity::block_snippet,
    error::{AppError, AppResult},
    sanitize,
};

const MAX_NAME_CHARS: usize = 64;
const MAX_ICON_CHARS: usize = 8;

#[derive(Serialize)]
pub struct Snippet {
    id: i32,
    name: String,
    icon: String,
    content: Value,
    sort_order: i32,
}

fn to_snippet(m: block_snippet::Model) -> Snippet {
    Snippet {
        id: m.id,
        name: m.name,
        icon: m.icon,
        content: serde_json::from_str(&m.content).unwrap_or_else(|_| Value::Array(vec![])),
        sort_order: m.sort_order,
    }
}

pub async fn list(State(st): State<AppState>, AuthUser(_): AuthUser) -> AppResult<Json<Vec<Snippet>>> {
    let rows = block_snippet::Entity::find()
        .order_by_asc(block_snippet::Column::SortOrder)
        .order_by_asc(block_snippet::Column::Id)
        .all(&st.db)
        .await?;
    Ok(Json(rows.into_iter().map(to_snippet).collect()))
}

fn validate(name: &str, icon: &str) -> AppResult<()> {
    if name.trim().is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest("名前が正しくありません".into()));
    }
    if icon.chars().count() > MAX_ICON_CHARS {
        return Err(AppError::BadRequest("アイコンが長すぎます".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
    #[serde(default)]
    icon: String,
    content: Value,
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<Snippet>> {
    validate(&req.name, &req.icon)?;
    let content = sanitize::sanitize_blocks(req.content).map_err(|e| AppError::BadRequest(e.into()))?;
    let last = block_snippet::Entity::find()
        .order_by_desc(block_snippet::Column::SortOrder)
        .one(&st.db)
        .await?;
    let now = Utc::now();
    let m = block_snippet::ActiveModel {
        name: Set(req.name),
        icon: Set(req.icon),
        content: Set(content.to_string()),
        sort_order: Set(last.map_or(0, |l| l.sort_order + 1)),
        created_by: Set(Some(cu.user.id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&st.db)
    .await?;
    Ok(Json(to_snippet(m)))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    icon: Option<String>,
    content: Option<Value>,
}

pub async fn update(
    State(st): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<Snippet>> {
    let m = block_snippet::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    validate(
        req.name.as_deref().unwrap_or(&m.name),
        req.icon.as_deref().unwrap_or(&m.icon),
    )?;
    let mut am: block_snippet::ActiveModel = m.into();
    if let Some(n) = req.name {
        am.name = Set(n);
    }
    if let Some(i) = req.icon {
        am.icon = Set(i);
    }
    if let Some(c) = req.content {
        let c = sanitize::sanitize_blocks(c).map_err(|e| AppError::BadRequest(e.into()))?;
        am.content = Set(c.to_string());
    }
    am.updated_at = Set(Utc::now());
    let m = am.update(&st.db).await?;
    Ok(Json(to_snippet(m)))
}

pub async fn delete(
    State(st): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let n = block_snippet::Entity::delete_by_id(id).exec(&st.db).await?.rows_affected;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "deleted": n })))
}
