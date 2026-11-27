//! ログインしていない人（ゲスト）からの書き込みに共通の部品：回数制限、送信元の確認、訪問者 Cookie。
//!
//! ゲストコメントとリアクションで使う。IP を含む記録は GUEST_IP_RETENTION_DAYS（既定 90日）で消す。

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::{HeaderMap, header};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

use crate::{
    AppState,
    auth::{random_token, sha256_hex},
    error::{AppError, AppResult},
};

/// 訪問者 Cookie（リアクションの重複を防ぐ）。値そのものは保存せず、SHA-256 だけを DB に入れる
pub const VISITOR_COOKIE: &str = "brikrs_vid";
const VISITOR_COOKIE_DAYS: i64 = 365;

/// 一定時間ごとに数え直す、キー（IP など）ごとの回数制限
pub struct WindowLimiter {
    max: u32,
    window: Duration,
    map: Mutex<HashMap<String, (u32, Instant)>>,
}

impl WindowLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self { max, window, map: Mutex::default() }
    }

    /// まだ上限に達していないか（数えない）
    pub fn check(&self, key: &str) -> bool {
        let map = self.map.lock().unwrap();
        map.get(key)
            .is_none_or(|(n, t)| t.elapsed() >= self.window || *n < self.max)
    }

    /// 1回数える
    pub fn hit(&self, key: &str) {
        let mut map = self.map.lock().unwrap();
        if map.len() > 10_000 {
            map.retain(|_, (_, t)| t.elapsed() < self.window);
        }
        let e = map.entry(key.to_string()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= self.window {
            *e = (0, Instant::now());
        }
        e.0 += 1;
    }

    /// 上限に達していなければ数えて true
    pub fn allow(&self, key: &str) -> bool {
        if !self.check(key) {
            return false;
        }
        self.hit(key);
        true
    }
}

/// 他のサイトのページから送らせた書き込みを断る。
/// ブラウザが付ける Sec-Fetch-Site を見て、無ければ Origin を PUBLIC_URL と比べる（PUBLIC_URL が無ければ Host と比べる）
pub fn check_same_origin(headers: &HeaderMap) -> AppResult<()> {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = get("sec-fetch-site") {
        return if matches!(site, "same-origin" | "none") {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        };
    }
    let Some(origin) = get(header::ORIGIN.as_str()) else {
        // 古いブラウザや、ブラウザ以外のクライアント。ほかの対策（トークン・回数制限）に任せる
        return Ok(());
    };
    let origin_host = url::Url::parse(origin).ok().and_then(|u| host_port(&u));
    let expected = match std::env::var("PUBLIC_URL").ok().filter(|s| !s.trim().is_empty()) {
        Some(p) => url::Url::parse(p.trim()).ok().and_then(|u| host_port(&u)),
        None => get(header::HOST.as_str()).map(|h| h.to_ascii_lowercase()),
    };
    match (origin_host, expected) {
        (Some(o), Some(e)) if o == e => Ok(()),
        // 比べる相手が分からなければ通す（設定の無い開発環境など）
        (_, None) => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

/// URL の「ホスト[:ポート]」（既定のポートは省く。Host ヘッダーと同じ形）
fn host_port(u: &url::Url) -> Option<String> {
    let host = u.host_str()?.to_ascii_lowercase();
    Some(match u.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

/// 訪問者 Cookie の値のハッシュ（Cookie が無ければ None）
pub fn visitor_hash(jar: &CookieJar) -> Option<String> {
    jar.get(VISITOR_COOKIE)
        .map(|c| c.value())
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(sha256_hex)
}

/// 訪問者 Cookie のハッシュ。無ければ新しく発行して jar に足す
pub fn ensure_visitor(st: &AppState, jar: CookieJar) -> (CookieJar, String) {
    if let Some(h) = visitor_hash(&jar) {
        return (jar, h);
    }
    let token = random_token();
    let hash = sha256_hex(&token);
    let cookie = Cookie::build((VISITOR_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(st.cookie_secure)
        .max_age(cookie::time::Duration::days(VISITOR_COOKIE_DAYS));
    (jar.add(cookie), hash)
}

/// IP・User-Agent を残す日数（0 なら無期限）
pub fn ip_retention_days() -> i64 {
    std::env::var("GUEST_IP_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n: &i64| *n >= 0)
        .unwrap_or(90)
}

pub fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().filter(|c| !c.is_control()).take(255).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_counts_per_key() {
        let l = WindowLimiter::new(2, Duration::from_secs(60));
        assert!(l.allow("a"));
        assert!(l.allow("a"));
        assert!(!l.allow("a"));
        assert!(l.allow("b"));
        // check は数えない
        assert!(l.check("b"));
        assert!(l.check("b"));
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    #[test]
    fn same_origin_by_fetch_metadata() {
        assert!(check_same_origin(&headers(&[("sec-fetch-site", "same-origin")])).is_ok());
        assert!(check_same_origin(&headers(&[("sec-fetch-site", "cross-site")])).is_err());
        assert!(check_same_origin(&headers(&[("sec-fetch-site", "same-site")])).is_err());
    }

    #[test]
    fn same_origin_by_origin_and_host() {
        // PUBLIC_URL はテストの環境に無い前提（あれば Host の代わりにそちらと比べる）
        if std::env::var("PUBLIC_URL").is_ok() {
            return;
        }
        let ok = headers(&[("origin", "http://localhost:8080"), ("host", "localhost:8080")]);
        assert!(check_same_origin(&ok).is_ok());
        let ng = headers(&[("origin", "https://evil.example"), ("host", "localhost:8080")]);
        assert!(check_same_origin(&ng).is_err());
    }
}
