//! ユーザーのグループ（ページの公開範囲に使う）。作成・削除・メンバーの出し入れは管理者だけ
//!
//! 管理者はグループではなく users.is_admin で、どのページも見られる（画面では「管理者」という特別なグループに見せる）

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, SqlErr, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::{AdminUser, AuthUser},
    entity::{page_group, user, user_group, user_group_member},
    error::{AppError, AppResult},
};

const MAX_NAME_CHARS: usize = 64;

#[derive(Serialize)]
pub struct GroupItem {
    id: i32,
    name: String,
    /// 見ている人が所属しているか
    member: bool,
    /// 所属しているユーザー（管理者にだけ返す）
    #[serde(skip_serializing_if = "Option::is_none")]
    user_ids: Option<Vec<i32>>,
    /// このグループを公開範囲に選んでいるページの数（管理者にだけ返す）
    #[serde(skip_serializing_if = "Option::is_none")]
    page_count: Option<u64>,
}

/// グループの一覧（名前順）。公開範囲を選ぶときに使うので、ログインしている人なら誰でも見られる
pub async fn list(State(st): State<AppState>, AuthUser(cu): AuthUser) -> AppResult<Json<Vec<GroupItem>>> {
    let admin = cu.user.is_admin;
    let groups = user_group::Entity::find()
        .order_by_asc(user_group::Column::Name)
        .all(&st.db)
        .await?;
    let members: Vec<(i32, i32)> = user_group_member::Entity::find()
        .select_only()
        .columns([user_group_member::Column::GroupId, user_group_member::Column::UserId])
        .into_tuple()
        .all(&st.db)
        .await?;
    let mut by_group: HashMap<i32, Vec<i32>> = HashMap::new();
    for (g, u) in members {
        by_group.entry(g).or_default().push(u);
    }
    let mut page_counts: HashMap<i32, u64> = HashMap::new();
    if admin {
        let rows: Vec<i32> = page_group::Entity::find()
            .select_only()
            .column(page_group::Column::GroupId)
            .into_tuple()
            .all(&st.db)
            .await?;
        for g in rows {
            *page_counts.entry(g).or_default() += 1;
        }
    }
    Ok(Json(
        groups
            .into_iter()
            .map(|g| {
                let mut users = by_group.remove(&g.id).unwrap_or_default();
                users.sort_unstable();
                GroupItem {
                    member: users.contains(&cu.user.id),
                    user_ids: admin.then_some(users),
                    page_count: admin.then(|| page_counts.get(&g.id).copied().unwrap_or(0)),
                    id: g.id,
                    name: g.name,
                }
            })
            .collect(),
    ))
}

fn clean_name(name: &str) -> AppResult<String> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "グループ名は1〜{MAX_NAME_CHARS}文字にしてください"
        )));
    }
    if name == "管理者" {
        return Err(AppError::BadRequest(
            "「管理者」は特別なグループの名前なので使えません".into(),
        ));
    }
    Ok(name)
}

fn duplicate(e: &sea_orm::DbErr) -> bool {
    matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

#[derive(Deserialize)]
pub struct CreateReq {
    name: String,
}

pub async fn create(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<Value>> {
    let name = clean_name(&req.name)?;
    let g = user_group::ActiveModel {
        name: Set(name),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&st.db)
    .await
    .map_err(|e| {
        if duplicate(&e) {
            AppError::BadRequest("同じ名前のグループがあります".into())
        } else {
            e.into()
        }
    })?;
    Ok(Json(json!({ "id": g.id, "name": g.name })))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    /// 所属するユーザー（置き換え）
    user_ids: Option<Vec<i32>>,
}

pub async fn update(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<Value>> {
    let g = user_group::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let txn = st.db.begin().await?;
    if let Some(name) = req.name {
        let name = clean_name(&name)?;
        let mut am: user_group::ActiveModel = g.into();
        am.name = Set(name);
        am.update(&txn).await.map_err(|e| {
            if duplicate(&e) {
                AppError::BadRequest("同じ名前のグループがあります".into())
            } else {
                e.into()
            }
        })?;
    }
    if let Some(mut ids) = req.user_ids {
        ids.sort_unstable();
        ids.dedup();
        if !ids.is_empty() {
            let found = user::Entity::find()
                .filter(user::Column::Id.is_in(ids.clone()))
                .count(&txn)
                .await?;
            if found != ids.len() as u64 {
                return Err(AppError::BadRequest("存在しないユーザーが含まれています".into()));
            }
        }
        user_group_member::Entity::delete_many()
            .filter(user_group_member::Column::GroupId.eq(id))
            .exec(&txn)
            .await?;
        if !ids.is_empty() {
            let rows: Vec<user_group_member::ActiveModel> = ids
                .into_iter()
                .map(|u| user_group_member::ActiveModel { group_id: Set(id), user_id: Set(u) })
                .collect();
            user_group_member::Entity::insert_many(rows).exec(&txn).await?;
        }
    }
    txn.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// グループを削除する。このグループだけに公開していたページは、管理者だけが見られるようになる
pub async fn delete(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let res = user_group::Entity::delete_by_id(id).exec(&st.db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}
