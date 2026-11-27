use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{PublicUser, monitor::record_login, totp};
use crate::{
    AppState,
    auth::{self, AuthUser, MaybeUser, SESSION_COOKIE, SESSION_TTL_DAYS},
    entity::{session, user},
    error::{AppError, AppResult},
};

pub async fn me(MaybeUser(cu): MaybeUser) -> Json<Value> {
    match cu {
        // バージョンはログインしている人にだけ返す（攻撃の手がかりを閲覧者に渡さない）
        Some(cu) => Json(json!({
            "user": PublicUser::from(cu.user),
            "csrf_token": cu.csrf_token,
            "version": crate::VERSION,
        })),
        None => Json(json!({ "user": null })),
    }
}

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

pub async fn login(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<LoginReq>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let ip = auth::client_ip(addr, &headers, st.trust_proxy);
    let keys = [
        format!("ip:{ip}"),
        format!("user:{}", req.username.to_lowercase()),
    ];
    if st.limiter.is_blocked(&keys) {
        record_login(&st.db, &req.username, &ip, &headers, "blocked", false).await;
        return Err(AppError::TooManyRequests);
    }

    let found = user::Entity::find()
        .filter(user::Column::Username.eq(req.username.as_str()))
        .one(&st.db)
        .await?;
    let hash = found
        .as_ref()
        .map(|u| u.password_hash.clone())
        .unwrap_or_else(|| auth::dummy_hash().to_string());
    let ok = auth::verify_password(req.password, hash).await;
    let Some(u) = found.filter(|_| ok) else {
        st.limiter.record_failure(&keys);
        record_login(&st.db, &req.username, &ip, &headers, "password", false).await;
        tracing::warn!(%ip, username = %req.username, "login failed");
        return Err(AppError::BadRequest(
            "ユーザー名またはパスワードが違います".into(),
        ));
    };

    // パスワードが通ったことは記録する（2段階認証が有効なら、ログインの完了はコードの確認後）
    record_login(&st.db, &u.username, &ip, &headers, "password", true).await;

    // 2段階認証が有効なら、まだセッションを作らずにコードを求める。
    // 失敗回数のリセットもコードが通ってから行う
    if u.totp_secret.is_some() {
        let token = st.mfa_pending.start(u.id);
        tracing::info!(%ip, username = %u.username, "password ok, waiting for 2fa code");
        return Ok((
            jar,
            Json(json!({ "mfa_required": true, "mfa_token": token })),
        ));
    }
    st.limiter.reset(&keys);
    start_session(&st, jar, u, &ip).await
}

#[derive(Deserialize)]
pub struct LoginTotpReq {
    mfa_token: String,
    code: String,
}

/// ログインの2段目：認証アプリのコード（またはリカバリーコード）
pub async fn login_totp(
    State(st): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<LoginTotpReq>,
) -> AppResult<(CookieJar, Json<Value>)> {
    let ip = auth::client_ip(addr, &headers, st.trust_proxy);
    let expired = || {
        AppError::BadRequest("確認の有効期限が切れました。もう一度ログインしてください".into())
    };
    let user_id = st.mfa_pending.attempt(&req.mfa_token).ok_or_else(expired)?;
    let u = user::Entity::find_by_id(user_id)
        .one(&st.db)
        .await?
        .ok_or_else(expired)?;
    let keys = [
        format!("ip:{ip}"),
        format!("user:{}", u.username.to_lowercase()),
    ];
    if st.limiter.is_blocked(&keys) {
        record_login(&st.db, &u.username, &ip, &headers, "blocked", false).await;
        return Err(AppError::TooManyRequests);
    }
    if !totp::check_second_factor(&st.db, &u, &req.code).await? {
        st.limiter.record_failure(&keys);
        record_login(&st.db, &u.username, &ip, &headers, "totp", false).await;
        tracing::warn!(%ip, username = %u.username, "2fa failed");
        return Err(AppError::BadRequest("確認コードが違います".into()));
    }
    st.mfa_pending.finish(&req.mfa_token);
    st.limiter.reset(&keys);
    record_login(&st.db, &u.username, &ip, &headers, "totp", true).await;
    start_session(&st, jar, u, &ip).await
}

async fn start_session(
    st: &AppState,
    jar: CookieJar,
    u: user::Model,
    ip: &str,
) -> AppResult<(CookieJar, Json<Value>)> {
    // 期限切れセッションの掃除はログインのついでに行う
    session::Entity::delete_many()
        .filter(session::Column::ExpiresAt.lt(Utc::now()))
        .exec(&st.db)
        .await?;

    let token = auth::random_token();
    let csrf = auth::random_token();
    let now = Utc::now();
    session::ActiveModel {
        id: Set(auth::sha256_hex(&token)),
        user_id: Set(u.id),
        csrf_token: Set(csrf.clone()),
        expires_at: Set(now + Duration::days(SESSION_TTL_DAYS)),
        created_at: Set(now),
    }
    .insert(&st.db)
    .await?;

    let cookie = Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(st.cookie_secure)
        .path("/")
        .max_age(cookie::time::Duration::days(SESSION_TTL_DAYS));
    tracing::info!(%ip, username = %u.username, "login");
    Ok((
        jar.add(cookie),
        Json(json!({ "user": PublicUser::from(u), "csrf_token": csrf })),
    ))
}

pub async fn logout(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<Value>)> {
    session::Entity::delete_by_id(cu.session_id)
        .exec(&st.db)
        .await?;
    Ok((
        jar.remove(Cookie::build(SESSION_COOKIE).path("/")),
        Json(json!({ "ok": true })),
    ))
}

/// 選べるカラーテーマ（フロントの theme.ts と揃える）
const THEMES: &[&str] = &["auto", "light", "dark"];

#[derive(Deserialize)]
pub struct ThemeReq {
    theme: String,
}

pub async fn set_theme(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<ThemeReq>,
) -> AppResult<Json<PublicUser>> {
    if !THEMES.contains(&req.theme.as_str()) {
        return Err(AppError::BadRequest("不明なテーマです".into()));
    }
    let mut am: user::ActiveModel = cu.user.into();
    am.theme = Set(Some(req.theme));
    Ok(Json(am.update(&st.db).await?.into()))
}

#[derive(Deserialize)]
pub struct ChangePasswordReq {
    current_password: String,
    new_password: String,
}

pub async fn change_password(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<ChangePasswordReq>,
) -> AppResult<Json<Value>> {
    if !auth::verify_password(req.current_password, cu.user.password_hash.clone()).await {
        return Err(AppError::BadRequest("現在のパスワードが違います".into()));
    }
    auth::validate_password_strength(&req.new_password)?;
    let hash = auth::hash_password(req.new_password).await?;
    let user_id = cu.user.id;
    let mut am: user::ActiveModel = cu.user.into();
    am.password_hash = Set(hash);
    am.update(&st.db).await?;
    // 他の端末のセッションは無効にする
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(user_id))
        .filter(session::Column::Id.ne(cu.session_id))
        .exec(&st.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
