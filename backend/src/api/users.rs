use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::Deserialize;
use serde_json::{Value, json};

use super::PublicUser;
use crate::{
    AppState,
    auth::{self, AdminUser},
    entity::{session, user},
    error::{AppError, AppResult},
};

pub async fn list(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
) -> AppResult<Json<Vec<PublicUser>>> {
    let users = user::Entity::find()
        .order_by_asc(user::Column::Id)
        .all(&st.db)
        .await?;
    Ok(Json(users.into_iter().map(PublicUser::from).collect()))
}

pub fn validate_username(name: &str) -> AppResult<()> {
    let ok = (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if ok {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "ユーザー名は英数字と _ - . の64文字以内にしてください".into(),
        ))
    }
}

fn validate_display_name(name: &str) -> AppResult<()> {
    if name.trim().is_empty() || name.chars().count() > 128 {
        return Err(AppError::BadRequest(
            "表示名は1〜128文字にしてください".into(),
        ));
    }
    Ok(())
}

pub async fn find_user(
    db: &sea_orm::DatabaseConnection,
    username: &str,
) -> AppResult<Option<user::Model>> {
    Ok(user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await?)
}

/// ユーザー作成。CLI からも使う
pub async fn create_user(
    db: &sea_orm::DatabaseConnection,
    username: &str,
    display_name: &str,
    password: String,
    is_admin: bool,
) -> AppResult<user::Model> {
    validate_username(username)?;
    validate_display_name(display_name)?;
    auth::validate_password_strength(&password)?;
    let exists = user::Entity::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await?
        .is_some();
    if exists {
        return Err(AppError::BadRequest(
            "そのユーザー名は既に使われています".into(),
        ));
    }
    let hash = auth::hash_password(password).await?;
    Ok(user::ActiveModel {
        username: Set(username.to_string()),
        display_name: Set(display_name.trim().to_string()),
        password_hash: Set(hash),
        is_admin: Set(is_admin),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await?)
}

#[derive(Deserialize)]
pub struct CreateReq {
    username: String,
    display_name: String,
    password: String,
    #[serde(default)]
    is_admin: bool,
}

pub async fn create(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<PublicUser>> {
    let u = create_user(
        &st.db,
        &req.username,
        &req.display_name,
        req.password,
        req.is_admin,
    )
    .await?;
    Ok(Json(u.into()))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    display_name: Option<String>,
    is_admin: Option<bool>,
    /// 管理者によるパスワード再設定
    password: Option<String>,
    /// 認証アプリを失くした人の2段階認証を解除する
    #[serde(default)]
    reset_totp: bool,
}

pub async fn update(
    State(st): State<AppState>,
    AdminUser(me): AdminUser,
    Path(id): Path<i32>,
    Json(req): Json<UpdateReq>,
) -> AppResult<Json<PublicUser>> {
    let u = user::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut am: user::ActiveModel = u.into();
    if let Some(name) = req.display_name {
        validate_display_name(&name)?;
        am.display_name = Set(name.trim().to_string());
    }
    if let Some(admin) = req.is_admin {
        if id == me.user.id && !admin {
            return Err(AppError::BadRequest(
                "自分自身の管理者権限は外せません".into(),
            ));
        }
        am.is_admin = Set(admin);
    }
    if req.reset_totp && id == me.user.id {
        return Err(AppError::BadRequest(
            "自分の2段階認証は「アカウント」から解除してください".into(),
        ));
    }
    let reset_password = req.password.is_some();
    if let Some(pw) = req.password {
        auth::validate_password_strength(&pw)?;
        am.password_hash = Set(auth::hash_password(pw).await?);
    }
    let u = am.update(&st.db).await?;
    if req.reset_totp {
        super::totp::clear_totp(&st.db, id).await?;
        tracing::info!(username = %u.username, by = %me.user.username, "totp reset by admin");
    }
    let u = user::Entity::find_by_id(id)
        .one(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    if reset_password || req.reset_totp {
        session::Entity::delete_many()
            .filter(session::Column::UserId.eq(id))
            .exec(&st.db)
            .await?;
    }
    Ok(Json(u.into()))
}

pub async fn delete(
    State(st): State<AppState>,
    AdminUser(me): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    if id == me.user.id {
        return Err(AppError::BadRequest("自分自身は削除できません".into()));
    }
    let res = user::Entity::delete_by_id(id).exec(&st.db).await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "ok": true })))
}
