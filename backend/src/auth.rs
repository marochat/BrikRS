use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::anyhow;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, Method, request::Parts},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use sha2::{Digest, Sha256};

use crate::{
    AppState,
    entity::{session, user},
    error::{AppError, AppResult},
};

pub const SESSION_COOKIE: &str = "brikrs_session";
pub const SESSION_TTL_DAYS: i64 = 14;
pub const CSRF_HEADER: &str = "x-csrf-token";

// ---------- パスワード ----------

pub async fn hash_password(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| anyhow!("hash error: {e}"))
    })
    .await
    .map_err(|e| anyhow!(e))?
    .map_err(AppError::from)
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// 存在しないユーザーでもハッシュ検証を走らせ、応答時間でユーザーの有無が分からないようにする
pub fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| {
        Argon2::default()
            .hash_password(b"dummy-password-for-timing")
            .expect("dummy hash")
            .to_string()
    })
}

pub fn validate_password_strength(pw: &str) -> AppResult<()> {
    if pw.chars().count() < 10 {
        return Err(AppError::BadRequest(
            "パスワードは10文字以上にしてください".into(),
        ));
    }
    if pw.len() > 256 {
        return Err(AppError::BadRequest("パスワードが長すぎます".into()));
    }
    Ok(())
}

// ---------- トークン ----------

pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS RNG unavailable");
    hex::encode(buf)
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------- ログイン失敗の回数制限 ----------

const LOGIN_MAX_FAILURES: u32 = 10;
const LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl LoginLimiter {
    pub fn is_blocked(&self, keys: &[String]) -> bool {
        let map = self.failures.lock().unwrap();
        keys.iter().any(|k| {
            map.get(k)
                .is_some_and(|(n, t)| *n >= LOGIN_MAX_FAILURES && t.elapsed() < LOGIN_WINDOW)
        })
    }

    pub fn record_failure(&self, keys: &[String]) {
        let mut map = self.failures.lock().unwrap();
        if map.len() > 10_000 {
            map.retain(|_, (_, t)| t.elapsed() < LOGIN_WINDOW);
        }
        for k in keys {
            let e = map.entry(k.clone()).or_insert((0, Instant::now()));
            if e.1.elapsed() >= LOGIN_WINDOW {
                *e = (0, Instant::now());
            }
            e.0 += 1;
        }
    }

    pub fn reset(&self, keys: &[String]) {
        let mut map = self.failures.lock().unwrap();
        for k in keys {
            map.remove(k);
        }
    }
}

// ---------- 2段階認証のコード待ち ----------

const MFA_TTL: Duration = Duration::from_secs(5 * 60);
const MFA_MAX_ATTEMPTS: u32 = 5;

/// パスワード確認が済んだログインを、コード入力まで覚えておく（キーはトークンの SHA-256）。
/// 再起動で消えても、パスワードからやり直すだけなのでメモリに置く
#[derive(Default)]
pub struct MfaPending {
    map: Mutex<HashMap<String, (i32, Instant, u32)>>,
}

impl MfaPending {
    /// 新しいトークンを発行する
    pub fn start(&self, user_id: i32) -> String {
        let token = random_token();
        let mut map = self.map.lock().unwrap();
        map.retain(|_, (_, t, _)| t.elapsed() < MFA_TTL);
        map.insert(sha256_hex(&token), (user_id, Instant::now(), 0));
        token
    }

    /// 有効なトークンならユーザー id。試行のたびに回数を数え、上限を超えたら無効にする
    pub fn attempt(&self, token: &str) -> Option<i32> {
        let mut map = self.map.lock().unwrap();
        let key = sha256_hex(token);
        let (user_id, t, n) = map.get_mut(&key)?;
        if t.elapsed() >= MFA_TTL || *n >= MFA_MAX_ATTEMPTS {
            map.remove(&key);
            return None;
        }
        *n += 1;
        Some(*user_id)
    }

    pub fn finish(&self, token: &str) {
        self.map.lock().unwrap().remove(&sha256_hex(token));
    }
}

pub fn client_ip(addr: SocketAddr, headers: &HeaderMap, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            return ip.trim().to_string();
        }
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
        {
            return ip.trim().to_string();
        }
    }
    addr.ip().to_string()
}

// ---------- リクエストからのログインユーザー取得 ----------

#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub user: user::Model,
    pub csrf_token: String,
    /// sessions.id（= クッキー値の SHA-256）
    pub session_id: String,
}

async fn load_current_user(parts: &Parts, state: &AppState) -> AppResult<Option<CurrentUser>> {
    let jar = CookieJar::from_headers(&parts.headers);
    let Some(cookie) = jar.get(SESSION_COOKIE) else {
        return Ok(None);
    };
    let sid = sha256_hex(cookie.value());
    let Some(sess) = session::Entity::find_by_id(sid.clone())
        .filter(session::Column::ExpiresAt.gt(Utc::now()))
        .one(&state.db)
        .await?
    else {
        return Ok(None);
    };
    let Some(u) = user::Entity::find_by_id(sess.user_id).one(&state.db).await? else {
        return Ok(None);
    };
    Ok(Some(CurrentUser {
        user: u,
        csrf_token: sess.csrf_token,
        session_id: sid,
    }))
}

/// ログインしていれば Some。公開ページの閲覧など、ログイン任意のAPI用
pub struct MaybeUser(pub Option<CurrentUser>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        Ok(MaybeUser(load_current_user(parts, state).await?))
    }
}

/// ログイン必須。GET/HEAD 以外では CSRF トークンのヘッダーも検証する
pub struct AuthUser(pub CurrentUser);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let cu = load_current_user(parts, state)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !matches!(parts.method, Method::GET | Method::HEAD) {
            let sent = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !constant_time_eq(sent.as_bytes(), cu.csrf_token.as_bytes()) {
                return Err(AppError::Forbidden);
            }
        }
        Ok(AuthUser(cu))
    }
}

/// 管理者のみ
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let AuthUser(cu) = AuthUser::from_request_parts(parts, state).await?;
        if !cu.user.is_admin {
            return Err(AppError::Forbidden);
        }
        Ok(AdminUser(cu))
    }
}
