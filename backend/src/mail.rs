//! メール送信と、承認待ちのゲストコメントの通知。
//!
//! - 送信は SMTP over SSL（暗黙の TLS、既定ポート 465）。portalspa の mail.rs と同じ作りで、環境変数の名前もそろえている
//!   （SMTP_SERVER・SMTP_PORT・SMTP_USER・SMTP_PASSWD・MAIL_FROM）
//! - 通知の宛先は COMMENT_NOTIFY_TO（カンマ区切りで複数可）。サイト設定は誰でも読めるので、宛先は環境変数に置く
//! - 1件ごとには送らず、最初のコメントから少し待って、その間に届いた分も1通にまとめる。送ったあとしばらくは送らない
//!   （スパムが大量に来てもメールが溢れないように）
//! - 送れなくてもコメントの受け付けには影響させない（ログに警告を出すだけ）
//! - メールには IP アドレスを入れない（保存期間を過ぎても消せないため）

use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::{
    AppState,
    entity::{comment, page},
};

/// 最初のコメントから、まとめて送るまで待つ時間
const GATHER: Duration = Duration::from_secs(5 * 60);
/// 送ったあと、次を送らない時間
const COOLDOWN: Duration = Duration::from_secs(30 * 60);
/// メールに載せる承認待ちの件数
const LIST_MAX: u64 = 10;
/// 本文は先頭のこの文字数だけ載せる
const EXCERPT_CHARS: usize = 100;

#[derive(Clone, Debug)]
pub struct MailConfig {
    pub server: String,
    pub port: u16,
    pub user: String,
    pub passwd: String,
    pub from: String,
}

fn env(key: &str) -> String {
    std::env::var(key).map(|v| v.trim().to_string()).unwrap_or_default()
}

impl MailConfig {
    /// SMTP_SERVER・SMTP_USER が無ければ None（メールは送らない）
    pub fn from_env() -> Option<Self> {
        let server = env("SMTP_SERVER");
        let user = env("SMTP_USER");
        if server.is_empty() || user.is_empty() {
            return None;
        }
        let port = env("SMTP_PORT").parse().unwrap_or(465);
        let from = Some(env("MAIL_FROM")).filter(|s| !s.is_empty()).unwrap_or_else(|| user.clone());
        Some(Self { server, port, passwd: env("SMTP_PASSWD"), from, user })
    }
}

/// 承認待ちの通知の宛先（COMMENT_NOTIFY_TO、カンマ区切り）
pub fn notify_to() -> Vec<String> {
    env("COMMENT_NOTIFY_TO")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub async fn send(cfg: &MailConfig, to: &[String], subject: &str, body: &str) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let from = cfg
        .from
        .parse()
        .with_context(|| format!("MAIL_FROM（無ければ SMTP_USER）がメールアドレスの形ではありません: {}", cfg.from))?;
    let mut b = Message::builder().from(from).subject(subject).header(ContentType::TEXT_PLAIN);
    for t in to {
        b = b.to(t.parse().with_context(|| format!("宛先がメールアドレスの形ではありません: {t}"))?);
    }
    let msg = b.body(body.to_string())?;
    let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.server)?
        .port(cfg.port)
        .credentials(Credentials::new(cfg.user.clone(), cfg.passwd.clone()))
        .timeout(Some(Duration::from_secs(20)))
        .build();
    transport.send(msg).await?;
    Ok(())
}

/// 起動時のログ：通知が有効か（宛先の数だけ。アドレスは出さない）
pub fn log_config() {
    let to = notify_to();
    match (MailConfig::from_env(), to.is_empty()) {
        (Some(_), false) => tracing::info!("承認待ちコメントのメール通知：有効（宛先 {} 件）", to.len()),
        (None, false) => tracing::warn!("COMMENT_NOTIFY_TO があるのに SMTP_SERVER・SMTP_USER が未設定のため、メール通知は無効"),
        _ => tracing::info!("承認待ちコメントのメール通知：無効（COMMENT_NOTIFY_TO が未設定）"),
    }
}

// ---------- 承認待ちの通知 ----------

#[derive(Default)]
struct NotifyState {
    /// 送る予定が入っている
    scheduled: bool,
    last_sent: Option<Instant>,
}

fn state() -> &'static Mutex<NotifyState> {
    static S: OnceLock<Mutex<NotifyState>> = OnceLock::new();
    S.get_or_init(Mutex::default)
}

/// 次に送るまで待つ時間：まとめる時間と、前に送ってからの間隔の長い方
fn delay_until_send(last_sent_ago: Option<Duration>) -> Duration {
    let cooldown_left = last_sent_ago.map_or(Duration::ZERO, |ago| COOLDOWN.saturating_sub(ago));
    GATHER.max(cooldown_left)
}

/// ゲストのコメントを受け付けたときに呼ぶ。送る予定がまだ無ければ入れる（あればその1通にまとまる）
pub fn comment_received(st: &AppState) {
    let to = notify_to();
    let Some(cfg) = MailConfig::from_env() else { return };
    if to.is_empty() {
        return;
    }
    let delay = {
        let mut s = state().lock().unwrap();
        if s.scheduled {
            return;
        }
        s.scheduled = true;
        delay_until_send(s.last_sent.map(|t| t.elapsed()))
    };
    let st = st.clone();
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        // 送る内容を作る前に予定を外す（作っている間に届いたコメントは次の1通に回る）
        state().lock().unwrap().scheduled = false;
        match digest(&st).await {
            Ok(Some((subject, body))) => match send(&cfg, &to, &subject, &body).await {
                Ok(()) => {
                    state().lock().unwrap().last_sent = Some(Instant::now());
                    tracing::info!("承認待ちコメントの通知を送りました");
                }
                Err(e) => tracing::warn!("承認待ちコメントの通知を送れません: {e:#}"),
            },
            // 待っている間に承認・削除されていれば送らない
            Ok(None) => {}
            Err(e) => tracing::warn!("承認待ちコメントの通知を作れません: {e:?}"),
        }
    });
}

/// 承認待ちの1件（メール用）
struct Item {
    name: String,
    page_title: String,
    body: String,
    link_count: i16,
}

/// 通知の件名と本文。承認待ちが無ければ None
async fn digest(st: &AppState) -> Result<Option<(String, String)>, sea_orm::DbErr> {
    let db: &DatabaseConnection = &st.db;
    let live = || {
        comment::Column::PageId.in_subquery(
            sea_orm::sea_query::Query::select()
                .column(page::Column::Id)
                .from(page::Entity)
                .and_where(page::Column::DeletedAt.is_null())
                .to_owned(),
        )
    };
    let total = comment::Entity::find()
        .filter(comment::Column::Status.eq("pending"))
        .filter(live())
        .count(db)
        .await?;
    if total == 0 {
        return Ok(None);
    }
    let rows = comment::Entity::find()
        .filter(comment::Column::Status.eq("pending"))
        .filter(live())
        .order_by_desc(comment::Column::Id)
        .limit(LIST_MAX)
        .all(db)
        .await?;
    let titles: std::collections::HashMap<i32, String> = page::Entity::find()
        .select_only()
        .column(page::Column::Id)
        .column(page::Column::Title)
        .filter(page::Column::Id.is_in(rows.iter().map(|c| c.page_id).collect::<Vec<_>>()))
        .into_tuple::<(i32, String)>()
        .all(db)
        .await?
        .into_iter()
        .collect();
    let items: Vec<Item> = rows
        .into_iter()
        .map(|c| Item {
            page_title: titles.get(&c.page_id).cloned().unwrap_or_default(),
            name: c.author_name,
            body: c.body,
            link_count: c.link_count,
        })
        .collect();
    let site = crate::api::site::site_title(st);
    let admin_url = std::env::var("PUBLIC_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .map(|u| format!("{u}/admin/users#comments"));
    Ok(Some(compose(&site, total, &items, admin_url.as_deref())))
}

fn excerpt(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > EXCERPT_CHARS {
        format!("{}…", flat.chars().take(EXCERPT_CHARS).collect::<String>())
    } else {
        flat
    }
}

fn compose(site: &str, total: u64, items: &[Item], admin_url: Option<&str>) -> (String, String) {
    let subject = format!("[{site}] 承認待ちのコメントが {total} 件あります");
    let mut body = format!("{site} に、承認待ちのゲストコメントが {total} 件あります。\n\n");
    if total as usize > items.len() {
        body += &format!("新しいもの {} 件：\n\n", items.len());
    }
    for it in items {
        let title = if it.page_title.is_empty() { "無題" } else { &it.page_title };
        body += &format!("■ {} さん（「{}」へのコメント）", it.name, title);
        if it.link_count > 0 {
            body += &format!(" リンク {} 件", it.link_count);
        }
        body += &format!("\n{}\n\n", excerpt(&it.body));
    }
    match admin_url {
        Some(u) => body += &format!("承認・削除はこちら：\n{u}\n"),
        None => body += "承認・削除は、管理 › コメント から行ってください。\n",
    }
    body += "\n-- \nこのメールは BrikRS から自動で送っています。\n";
    (subject, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_gather_or_cooldown() {
        assert_eq!(delay_until_send(None), GATHER);
        // 送ってすぐなら、間隔が空くまで待つ
        assert_eq!(delay_until_send(Some(Duration::from_secs(60))), COOLDOWN - Duration::from_secs(60));
        // 間隔が空いていれば、まとめる時間だけ
        assert_eq!(delay_until_send(Some(COOLDOWN)), GATHER);
    }

    #[test]
    fn composes_digest() {
        let items = vec![
            Item { name: "Guest".into(), page_title: "hello".into(), body: "あ\n い".into(), link_count: 0 },
            Item { name: "Spam".into(), page_title: String::new(), body: "x".repeat(150), link_count: 2 },
        ];
        let (subject, body) = compose("BrikRS", 5, &items, Some("https://example.org/admin/users#comments"));
        assert_eq!(subject, "[BrikRS] 承認待ちのコメントが 5 件あります");
        assert!(body.contains("新しいもの 2 件"));
        assert!(body.contains("■ Guest さん（「hello」へのコメント）\nあ い\n"));
        assert!(body.contains("「無題」へのコメント） リンク 2 件"));
        assert!(body.contains(&format!("{}…", "x".repeat(EXCERPT_CHARS))));
        assert!(body.contains("https://example.org/admin/users#comments"));
    }
}
