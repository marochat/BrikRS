//! アクセス解析とログイン監視（管理者向け）。
//!
//! - 閲覧者の画面遷移ごとにページビューを記録する（編集者自身の閲覧は数えない）
//! - ログインの試行（パスワード・2段階認証、成功・失敗、回数制限で止めたもの）を記録する
//! - 管理者の画面向けに、期間を区切って集計する。日付は閲覧している人のタイムゾーンで区切る
//!
//! 記録は IP アドレスを含むので、定期的な掃除で古いものを消す（LOG_RETENTION_DAYS、既定 180日）。

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Mutex, OnceLock},
    time::{Duration as StdDuration, Instant},
};

use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode, header},
};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, Set, Statement,
    sea_query::Value as SqlValue,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::{self, AdminUser, MaybeUser},
    entity::{access_log, login_attempt, page},
    error::AppResult,
};

/// 1つの IP から、この件数以上の失敗があれば「要注意」として強調する（表示だけで、ブロックはしない）
const SUSPICIOUS_FAILURE_THRESHOLD: i64 = 5;
/// ページビューの記録を受け付ける上限（1つの IP から1分あたり）。水増しや DB の圧迫を防ぐ
const PAGEVIEW_PER_MINUTE: u32 = 60;
const MAX_DAYS: i64 = 365;

// 簡易な bot / クローラーの判定（User-Agent のキーワード）。集計からは除く
const BOT_KEYWORDS: &[&str] = &[
    "bot",
    "spider",
    "crawl",
    "slurp",
    "facebookexternalhit",
    "bingpreview",
    "curl",
    "wget",
    "python-requests",
    "headlesschrome",
    "postmanruntime",
    "google-inspectiontool",
    "preview",
];

fn is_bot(ua: &str) -> bool {
    let ua = ua.to_ascii_lowercase();
    BOT_KEYWORDS.iter().any(|k| ua.contains(k))
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(255).collect())
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(n).collect()
}

// ---------- ページビューの記録 ----------

#[derive(Default)]
struct PageviewLimiter(Mutex<HashMap<String, (u32, Instant)>>);

impl PageviewLimiter {
    fn allow(&self, ip: &str) -> bool {
        let mut map = self.0.lock().unwrap();
        if map.len() > 10_000 {
            map.retain(|_, (_, t)| t.elapsed() < StdDuration::from_secs(60));
        }
        let e = map.entry(ip.to_string()).or_insert((0, Instant::now()));
        if e.1.elapsed() >= StdDuration::from_secs(60) {
            *e = (0, Instant::now());
        }
        e.0 += 1;
        e.0 <= PAGEVIEW_PER_MINUTE
    }
}

fn pageview_limiter() -> &'static PageviewLimiter {
    static L: OnceLock<PageviewLimiter> = OnceLock::new();
    L.get_or_init(PageviewLimiter::default)
}

#[derive(Deserialize)]
pub struct PageviewReq {
    path: String,
    visitor_id: String,
    session_id: String,
    referrer: Option<String>,
}

fn valid_id(s: &str) -> bool {
    s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// このサイトの中のパスだけを記録する。`//host` と `/\host` はブラウザが別のサイトとして扱うので断る
/// （管理画面のアクセス解析でリンクになるため）
fn valid_path(s: &str) -> bool {
    s.starts_with('/')
        && !s.starts_with("//")
        && !s.contains('\\')
        && !s.chars().any(char::is_control)
}

/// 画面遷移ごとに呼ばれる（ログインしていなくても呼べる）。編集者自身の閲覧は数えない
pub async fn pageview(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<PageviewReq>,
) -> AppResult<StatusCode> {
    if cu.is_some()
        || !valid_path(&req.path)
        || req.path.len() > 255
        || !valid_id(&req.visitor_id)
        || !valid_id(&req.session_id)
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    let ip = auth::client_ip(addr, &headers, st.trust_proxy);
    if !pageview_limiter().allow(&ip) {
        return Ok(StatusCode::NO_CONTENT);
    }
    let ua = user_agent(&headers);
    access_log::ActiveModel {
        path: Set(truncate(&req.path, 255)),
        visitor_id: Set(req.visitor_id),
        session_id: Set(req.session_id),
        ip: Set(Some(ip)),
        is_bot: Set(ua.as_deref().is_some_and(is_bot)),
        user_agent: Set(ua),
        referrer: Set(req
            .referrer
            .filter(|r| !r.is_empty())
            .map(|r| truncate(&r, 255))),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&st.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- ログイン試行の記録 ----------

/// ログインの試行を記録する。記録に失敗してもログイン自体は止めない
pub async fn record_login(
    db: &impl ConnectionTrait,
    username: &str,
    ip: &str,
    headers: &HeaderMap,
    kind: &str,
    success: bool,
) {
    let res = login_attempt::ActiveModel {
        username: Set(truncate(username, 150)),
        ip: Set(Some(ip.to_string())),
        user_agent: Set(user_agent(headers)),
        kind: Set(kind.into()),
        success: Set(success),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await;
    if let Err(e) = res {
        tracing::warn!("ログイン試行を記録できません: {e}");
    }
}

// ---------- 集計 ----------

#[derive(Deserialize)]
pub struct PeriodQuery {
    #[serde(default = "default_days")]
    days: i64,
    /// 閲覧している人のタイムゾーン（UTC から東へ何分。日本なら 540）
    #[serde(default)]
    tz: i64,
}

fn default_days() -> i64 {
    30
}

/// 期間の始まり（閲覧している人の日付で days 日前の 0時）を UTC で。期間の初日・末日も返す
fn period(q: &PeriodQuery) -> (DateTime<Utc>, NaiveDate, NaiveDate, i64) {
    let days = q.days.clamp(1, MAX_DAYS);
    let tz = q.tz.clamp(-14 * 60, 14 * 60);
    let today = (Utc::now() + Duration::minutes(tz)).date_naive();
    let start = today - Duration::days(days - 1);
    let start_utc = start.and_hms_opt(0, 0, 0).unwrap().and_utc() - Duration::minutes(tz);
    (start_utc, start, today, tz)
}

fn stmt(sql: &str, values: Vec<SqlValue>) -> Statement {
    Statement::from_sql_and_values(DbBackend::MySql, sql, values)
}

async fn rows(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> AppResult<Vec<sea_orm::QueryResult>> {
    Ok(db.query_all_raw(stmt(sql, values)).await?)
}

async fn count(db: &impl ConnectionTrait, sql: &str, values: Vec<SqlValue>) -> AppResult<i64> {
    Ok(match db.query_one_raw(stmt(sql, values)).await? {
        Some(r) => r.try_get_by_index::<i64>(0)?,
        None => 0,
    })
}

#[derive(Serialize)]
struct TopPage {
    path: String,
    /// /p/<id> ならページのタイトルとアイコン
    title: Option<String>,
    icon: Option<String>,
    pageviews: i64,
    unique_visitors: i64,
}

/// アクセス解析（bot を除いた閲覧者の数字）
pub async fn access_summary(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Query(q): Query<PeriodQuery>,
) -> AppResult<Json<Value>> {
    let (start, start_date, end_date, tz) = period(&q);
    let db = &st.db;
    let s = || SqlValue::from(start.naive_utc());

    let total_pageviews = count(
        db,
        "SELECT COUNT(*) FROM access_logs WHERE is_bot = 0 AND created_at >= ?",
        vec![s()],
    )
    .await?;
    let unique_visitors = count(
        db,
        "SELECT COUNT(DISTINCT visitor_id) FROM access_logs WHERE is_bot = 0 AND created_at >= ?",
        vec![s()],
    )
    .await?;
    let sessions = count(
        db,
        "SELECT COUNT(DISTINCT session_id) FROM access_logs WHERE is_bot = 0 AND created_at >= ?",
        vec![s()],
    )
    .await?;
    let bot_pageviews = count(
        db,
        "SELECT COUNT(*) FROM access_logs WHERE is_bot = 1 AND created_at >= ?",
        vec![s()],
    )
    .await?;

    let daily: Vec<Value> = rows(
        db,
        "SELECT DATE(DATE_ADD(created_at, INTERVAL ? MINUTE)) AS d, COUNT(*) AS pv, COUNT(DISTINCT visitor_id) AS uv
         FROM access_logs WHERE is_bot = 0 AND created_at >= ? GROUP BY d ORDER BY d",
        vec![tz.into(), s()],
    )
    .await?
    .into_iter()
    .map(|r| -> AppResult<Value> {
        Ok(json!({
            "date": r.try_get::<NaiveDate>("", "d")?.to_string(),
            "pageviews": r.try_get::<i64>("", "pv")?,
            "unique_visitors": r.try_get::<i64>("", "uv")?,
        }))
    })
    .collect::<AppResult<_>>()?;

    let mut top_pages: Vec<TopPage> = rows(
        db,
        "SELECT path, COUNT(*) AS pv, COUNT(DISTINCT visitor_id) AS uv
         FROM access_logs WHERE is_bot = 0 AND created_at >= ? GROUP BY path ORDER BY pv DESC LIMIT 15",
        vec![s()],
    )
    .await?
    .into_iter()
    .map(|r| -> AppResult<TopPage> {
        Ok(TopPage {
            path: r.try_get("", "path")?,
            title: None,
            icon: None,
            pageviews: r.try_get("", "pv")?,
            unique_visitors: r.try_get("", "uv")?,
        })
    })
    .collect::<AppResult<_>>()?;
    // /p/<id> はページのタイトルに置き換えて読めるようにする
    let ids: Vec<i32> = top_pages
        .iter()
        .filter_map(|p| p.path.strip_prefix("/p/")?.parse().ok())
        .collect();
    if !ids.is_empty() {
        let titles: HashMap<i32, (String, String)> = page::Entity::find()
            .filter(sea_orm::ColumnTrait::is_in(&page::Column::Id, ids))
            .all(db)
            .await?
            .into_iter()
            .map(|p| (p.id, (p.title, p.icon)))
            .collect();
        for tp in &mut top_pages {
            if let Some((t, i)) = tp
                .path
                .strip_prefix("/p/")
                .and_then(|s| s.parse::<i32>().ok())
                .and_then(|id| titles.get(&id))
            {
                tp.title = Some(t.clone());
                tp.icon = Some(i.clone());
            }
        }
    }

    // 参照元（外部のサイトから来たもの。ホスト名ごと）
    let mut refs: HashMap<String, i64> = HashMap::new();
    for r in rows(
        db,
        "SELECT referrer, COUNT(*) AS n FROM access_logs
         WHERE is_bot = 0 AND created_at >= ? AND referrer IS NOT NULL GROUP BY referrer",
        vec![s()],
    )
    .await?
    {
        let url: String = r.try_get("", "referrer")?;
        let n: i64 = r.try_get("", "n")?;
        if let Some(host) = url
            .split_once("://")
            .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or(""))
            .filter(|h| !h.is_empty())
        {
            *refs.entry(host.to_ascii_lowercase()).or_default() += n;
        }
    }
    let own_host = std::env::var("PUBLIC_URL").ok().and_then(|u| {
        u.split_once("://")
            .map(|(_, r)| r.trim_end_matches('/').to_ascii_lowercase())
    });
    let mut referrers: Vec<(String, i64)> = refs
        .into_iter()
        .filter(|(h, _)| Some(h) != own_host.as_ref())
        .collect();
    referrers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    referrers.truncate(10);

    Ok(Json(json!({
        "start_date": start_date.to_string(),
        "end_date": end_date.to_string(),
        "total_pageviews": total_pageviews,
        "total_unique_visitors": unique_visitors,
        "total_sessions": sessions,
        "bot_pageviews": bot_pageviews,
        "daily": daily,
        "top_pages": top_pages,
        "referrers": referrers.into_iter().map(|(host, n)| json!({ "host": host, "pageviews": n })).collect::<Vec<_>>(),
    })))
}

/// ログイン監視（パスワードアタックの兆候）
pub async fn login_summary(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Query(q): Query<PeriodQuery>,
) -> AppResult<Json<Value>> {
    let (start, start_date, end_date, tz) = period(&q);
    let db = &st.db;
    let s = || SqlValue::from(start.naive_utc());

    let total = count(
        db,
        "SELECT COUNT(*) FROM login_attempts WHERE created_at >= ?",
        vec![s()],
    )
    .await?;
    let failures = count(
        db,
        "SELECT COUNT(*) FROM login_attempts WHERE success = 0 AND created_at >= ?",
        vec![s()],
    )
    .await?;
    let unique_failure_ips = count(
        db,
        "SELECT COUNT(DISTINCT ip) FROM login_attempts WHERE success = 0 AND created_at >= ?",
        vec![s()],
    )
    .await?;
    let unique_usernames = count(
        db,
        "SELECT COUNT(DISTINCT username) FROM login_attempts WHERE created_at >= ?",
        vec![s()],
    )
    .await?;

    let daily_failures: Vec<Value> = rows(
        db,
        "SELECT DATE(DATE_ADD(created_at, INTERVAL ? MINUTE)) AS d, COUNT(*) AS n
         FROM login_attempts WHERE success = 0 AND created_at >= ? GROUP BY d ORDER BY d",
        vec![tz.into(), s()],
    )
    .await?
    .into_iter()
    .map(|r| -> AppResult<Value> {
        Ok(json!({ "date": r.try_get::<NaiveDate>("", "d")?.to_string(), "failures": r.try_get::<i64>("", "n")? }))
    })
    .collect::<AppResult<_>>()?;

    // IP ごとの失敗の多い順（上位20件）。成功の数は別に数えて合わせる
    let successes_by_ip: HashMap<String, i64> = rows(
        db,
        "SELECT ip, COUNT(*) AS n FROM login_attempts WHERE success = 1 AND ip IS NOT NULL AND created_at >= ? GROUP BY ip",
        vec![s()],
    )
    .await?
    .into_iter()
    .filter_map(|r| Some((r.try_get("", "ip").ok()?, r.try_get("", "n").ok()?)))
    .collect();
    let suspicious_ips: Vec<Value> = rows(
        db,
        "SELECT ip, COUNT(*) AS n, COUNT(DISTINCT username) AS users, MAX(created_at) AS last_at
         FROM login_attempts WHERE success = 0 AND ip IS NOT NULL AND created_at >= ?
         GROUP BY ip ORDER BY n DESC LIMIT 20",
        vec![s()],
    )
    .await?
    .into_iter()
    .map(|r| -> AppResult<Value> {
        let ip: String = r.try_get("", "ip")?;
        let n: i64 = r.try_get("", "n")?;
        Ok(json!({
            "successes": successes_by_ip.get(&ip).copied().unwrap_or(0),
            "ip": ip,
            "failures": n,
            "distinct_usernames": r.try_get::<i64>("", "users")?,
            "last_attempt": r.try_get::<NaiveDateTime>("", "last_at")?.and_utc(),
            "suspicious": n >= SUSPICIOUS_FAILURE_THRESHOLD,
        }))
    })
    .collect::<AppResult<_>>()?;

    let recent_failures: Vec<Value> = rows(
        db,
        "SELECT created_at, username, ip, user_agent, kind FROM login_attempts
         WHERE success = 0 AND created_at >= ? ORDER BY created_at DESC, id DESC LIMIT 30",
        vec![s()],
    )
    .await?
    .into_iter()
    .map(|r| -> AppResult<Value> {
        Ok(json!({
            "created_at": r.try_get::<NaiveDateTime>("", "created_at")?.and_utc(),
            "username": r.try_get::<String>("", "username")?,
            "ip": r.try_get::<Option<String>>("", "ip")?,
            "user_agent": r.try_get::<Option<String>>("", "user_agent")?,
            "kind": r.try_get::<String>("", "kind")?,
        }))
    })
    .collect::<AppResult<_>>()?;

    Ok(Json(json!({
        "start_date": start_date.to_string(),
        "end_date": end_date.to_string(),
        "total_attempts": total,
        "total_failures": failures,
        "total_successes": total - failures,
        "unique_failure_ips": unique_failure_ips,
        "unique_attempted_usernames": unique_usernames,
        "suspicious_threshold": SUSPICIOUS_FAILURE_THRESHOLD,
        "daily_failures": daily_failures,
        "suspicious_ips": suspicious_ips,
        "recent_failures": recent_failures,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_bots() {
        assert!(is_bot("Mozilla/5.0 (compatible; Googlebot/2.1)"));
        assert!(is_bot("Twitterbot/1.0"));
        assert!(is_bot("curl/8.5.0"));
        assert!(!is_bot(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/140.0 Safari/537.36"
        ));
    }

    #[test]
    fn only_same_site_paths() {
        assert!(valid_path("/"));
        assert!(valid_path("/p/12?month=2026-10"));
        assert!(!valid_path("p/12"));
        assert!(!valid_path("//evil.example"));
        assert!(!valid_path("/\\evil.example"));
        assert!(!valid_path("/p/1\\x"));
        assert!(!valid_path("/p/1\n"));
    }

    #[test]
    fn period_in_viewer_timezone() {
        let (start, s, e, _) = period(&PeriodQuery { days: 7, tz: 540 });
        assert_eq!((e - s).num_days(), 6);
        // 日本時間の 0時 = UTC の前日 15時
        assert_eq!(start.format("%H:%M").to_string(), "15:00");
    }
}
