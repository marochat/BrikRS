//! 2段階認証（TOTP）の設定。ログイン時の確認は session.rs

use axum::{Json, extract::State};
use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    sea_query::{Expr, ExprTrait},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::{self, AuthUser},
    entity::{session, user},
    error::{AppError, AppResult},
    totp,
};

fn issuer() -> String {
    std::env::var("TOTP_ISSUER").unwrap_or_else(|_| "BrikRS".into())
}

fn recovery_hashes(u: &user::Model) -> Vec<String> {
    u.totp_recovery
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 認証アプリのコードか、未使用のリカバリーコードなら true。
/// 使ったコードは消費する（同じ区間の TOTP・同じリカバリーコードは二度と通らない）。
/// 同時に2つのリクエストが同じコードを使っても片方しか通らないよう、条件付きの UPDATE で消費する
pub async fn check_second_factor(
    db: &impl ConnectionTrait,
    u: &user::Model,
    code: &str,
) -> AppResult<bool> {
    let Some(secret) = &u.totp_secret else {
        return Ok(false);
    };
    if let Some(step) = totp::verify(secret, code, Utc::now().timestamp(), u.totp_last_step) {
        let res = user::Entity::update_many()
            .col_expr(user::Column::TotpLastStep, Expr::value(step))
            .filter(user::Column::Id.eq(u.id))
            .filter(
                Expr::col(user::Column::TotpLastStep)
                    .is_null()
                    .or(Expr::col(user::Column::TotpLastStep).lt(step)),
            )
            .exec(db)
            .await?;
        return Ok(res.rows_affected == 1);
    }

    let mut hashes = recovery_hashes(u);
    let h = totp::recovery_hash(code);
    let Some(i) = hashes.iter().position(|x| *x == h) else {
        return Ok(false);
    };
    hashes.remove(i);
    let res = user::Entity::update_many()
        .col_expr(
            user::Column::TotpRecovery,
            Expr::value(serde_json::to_string(&hashes).map_err(anyhow::Error::from)?),
        )
        .filter(user::Column::Id.eq(u.id))
        .filter(user::Column::TotpRecovery.eq(u.totp_recovery.clone()))
        .exec(db)
        .await?;
    if res.rows_affected == 1 {
        tracing::info!(username = %u.username, left = hashes.len(), "recovery code used");
    }
    Ok(res.rows_affected == 1)
}

/// ログイン中のユーザーがコードを打ち間違え続けられないよう、回数制限をかけて確認する
async fn require_code(st: &AppState, u: &user::Model, code: &str) -> AppResult<()> {
    let keys = [format!("totp:{}", u.id)];
    if st.limiter.is_blocked(&keys) {
        return Err(AppError::TooManyRequests);
    }
    if !check_second_factor(&st.db, u, code).await? {
        st.limiter.record_failure(&keys);
        return Err(AppError::BadRequest("確認コードが違います".into()));
    }
    st.limiter.reset(&keys);
    Ok(())
}

fn new_recovery_codes() -> AppResult<(Vec<String>, String)> {
    let codes = totp::generate_recovery_codes();
    let hashes: Vec<String> = codes.iter().map(|c| totp::recovery_hash(c)).collect();
    Ok((
        codes,
        serde_json::to_string(&hashes).map_err(anyhow::Error::from)?,
    ))
}

pub async fn status(AuthUser(cu): AuthUser) -> Json<Value> {
    Json(json!({
        "enabled": cu.user.totp_secret.is_some(),
        "recovery_codes_left": recovery_hashes(&cu.user).len(),
    }))
}

/// 新しい秘密鍵を作って返す。まだ保存せず、enable でコードが確認できてから保存する
pub async fn setup(AuthUser(cu): AuthUser) -> AppResult<Json<Value>> {
    if cu.user.totp_secret.is_some() {
        return Err(AppError::BadRequest("2段階認証は既に有効です".into()));
    }
    let secret = totp::generate_secret();
    let uri = totp::otpauth_uri(&issuer(), &cu.user.username, &secret);
    let svg = totp::qr_svg(&uri)?;
    Ok(Json(json!({ "secret": secret, "uri": uri, "qr_svg": svg })))
}

#[derive(Deserialize)]
pub struct EnableReq {
    secret: String,
    code: String,
}

pub async fn enable(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<EnableReq>,
) -> AppResult<Json<Value>> {
    if cu.user.totp_secret.is_some() {
        return Err(AppError::BadRequest("2段階認証は既に有効です".into()));
    }
    if req.secret.len() != 32 {
        return Err(AppError::BadRequest("秘密鍵の形式が不正です".into()));
    }
    let Some(step) = totp::verify(&req.secret, &req.code, Utc::now().timestamp(), None) else {
        return Err(AppError::BadRequest(
            "確認コードが違います。認証アプリに表示されている6桁を入力してください（端末の時刻がずれていないかも確認してください）".into(),
        ));
    };
    let (codes, hashes) = new_recovery_codes()?;
    user::Entity::update_many()
        .col_expr(user::Column::TotpSecret, Expr::value(req.secret))
        .col_expr(user::Column::TotpLastStep, Expr::value(step))
        .col_expr(user::Column::TotpRecovery, Expr::value(hashes))
        .filter(user::Column::Id.eq(cu.user.id))
        .exec(&st.db)
        .await?;
    // 2段階認証なしでログインした他の端末は追い出す
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(cu.user.id))
        .filter(session::Column::Id.ne(cu.session_id))
        .exec(&st.db)
        .await?;
    tracing::info!(username = %cu.user.username, "totp enabled");
    Ok(Json(json!({ "recovery_codes": codes })))
}

#[derive(Deserialize)]
pub struct DisableReq {
    password: String,
    code: String,
}

pub async fn disable(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<DisableReq>,
) -> AppResult<Json<Value>> {
    if cu.user.totp_secret.is_none() {
        return Err(AppError::BadRequest(
            "2段階認証は有効になっていません".into(),
        ));
    }
    if !auth::verify_password(req.password, cu.user.password_hash.clone()).await {
        return Err(AppError::BadRequest("パスワードが違います".into()));
    }
    require_code(&st, &cu.user, &req.code).await?;
    clear_totp(&st.db, cu.user.id).await?;
    tracing::info!(username = %cu.user.username, "totp disabled");
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct RegenerateReq {
    code: String,
}

/// リカバリーコードを作り直す（古いものは使えなくなる）
pub async fn regenerate_recovery(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<RegenerateReq>,
) -> AppResult<Json<Value>> {
    if cu.user.totp_secret.is_none() {
        return Err(AppError::BadRequest(
            "2段階認証は有効になっていません".into(),
        ));
    }
    require_code(&st, &cu.user, &req.code).await?;
    let (codes, hashes) = new_recovery_codes()?;
    user::Entity::update_many()
        .col_expr(user::Column::TotpRecovery, Expr::value(hashes))
        .filter(user::Column::Id.eq(cu.user.id))
        .exec(&st.db)
        .await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}

/// 2段階認証を解除する（本人の解除、管理者による解除、CLI で共用）
pub async fn clear_totp(db: &impl ConnectionTrait, user_id: i32) -> AppResult<()> {
    user::Entity::update_many()
        .col_expr(
            user::Column::TotpSecret,
            Expr::value(Option::<String>::None),
        )
        .col_expr(user::Column::TotpLastStep, Expr::value(Option::<i64>::None))
        .col_expr(
            user::Column::TotpRecovery,
            Expr::value(Option::<String>::None),
        )
        .filter(user::Column::Id.eq(user_id))
        .exec(db)
        .await?;
    Ok(())
}
