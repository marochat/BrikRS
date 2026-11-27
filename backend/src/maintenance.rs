//! 定期的な掃除：ゴミ箱の自動削除、履歴の保持数の上限、使われていない画像、期限切れのセッション、
//! ゲストコメント・リアクションの IP の消去と、古い承認待ちコメントの削除。
//!
//! `serve` 中は起動の少し後から一定間隔で動く。`brikrs maintenance` で1回だけ実行もできる。

use std::{collections::BTreeSet, path::Path, time::Duration as StdDuration};

use chrono::{Duration, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
    sea_query::{Expr, ExprTrait},
};

use crate::{
    api::trash,
    entity::{access_log, block_snippet, comment, login_attempt, page, page_revision, reaction, session, upload},
    error::{AppError, AppResult},
    refs,
};

const INTERVAL: StdDuration = StdDuration::from_secs(6 * 60 * 60);
const FIRST_RUN_DELAY: StdDuration = StdDuration::from_secs(60);
/// アップロード直後は本文の保存がまだ届いていないことがあるので、この時間は消さない
const UPLOAD_GRACE_HOURS: i64 = 24;
/// 本文を走査するときに一度に読む行数（メモリを抑えるため）
const SCAN_BATCH: u64 = 100;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// ゴミ箱に入れてから完全に削除するまでの日数（0 なら自動削除しない）
    pub trash_retention_days: i64,
    /// ページごとに残す履歴の数（0 なら無制限）
    pub revision_keep: u64,
    /// アクセスログ・ログイン試行の記録を残す日数（0 なら無期限）。IP アドレスを含むので既定 180日
    pub log_retention_days: i64,
    /// ゲストコメント・リアクションの IP と User-Agent を残す日数（0 なら無期限）。
    /// 過ぎたら IP などを消し、承認されないままのコメントは削除する。既定 90日
    pub guest_ip_retention_days: i64,
}

impl Config {
    pub fn from_env() -> Self {
        let num = |key: &str, default: i64| -> i64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .filter(|n: &i64| *n >= 0)
                .unwrap_or(default)
        };
        Self {
            trash_retention_days: num("TRASH_RETENTION_DAYS", 30),
            revision_keep: num("REVISION_KEEP", 100) as u64,
            log_retention_days: num("LOG_RETENTION_DAYS", 180),
            guest_ip_retention_days: crate::guest::ip_retention_days(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub purged_pages: u64,
    pub pruned_revisions: u64,
    pub removed_uploads: u64,
    pub expired_sessions: u64,
    pub old_logs: u64,
    pub guest_ips_cleared: u64,
    pub stale_pending_comments: u64,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "ゴミ箱から削除 {} ページ, 古い履歴 {} 件, 未使用の画像 {} 件, 期限切れセッション {} 件, 古いアクセス・ログイン記録 {} 件, \
             ゲストの IP の消去 {} 件, 古い承認待ちコメント {} 件",
            self.purged_pages,
            self.pruned_revisions,
            self.removed_uploads,
            self.expired_sessions,
            self.old_logs,
            self.guest_ips_cleared,
            self.stale_pending_comments
        )
    }
}

pub fn spawn(db: DatabaseConnection, upload_dir: std::path::PathBuf, cfg: Config) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_RUN_DELAY).await;
        let mut tick = tokio::time::interval(INTERVAL);
        loop {
            tick.tick().await;
            match run(&db, &upload_dir, cfg).await {
                Ok(r) => tracing::info!("maintenance: {r}"),
                Err(e) => tracing::error!("maintenance failed: {e:?}"),
            }
        }
    });
}

pub async fn run(db: &DatabaseConnection, upload_dir: &Path, cfg: Config) -> AppResult<Report> {
    // 先にページと履歴を消しておくと、それらだけが使っていた画像も同じ回で消せる
    Ok(Report {
        purged_pages: purge_old_trash(db, cfg.trash_retention_days).await?,
        pruned_revisions: prune_revisions(db, cfg.revision_keep).await?,
        removed_uploads: remove_unused_uploads(db, upload_dir).await?,
        expired_sessions: session::Entity::delete_many()
            .filter(session::Column::ExpiresAt.lte(Utc::now()))
            .exec(db)
            .await?
            .rows_affected,
        old_logs: delete_old_logs(db, cfg.log_retention_days).await?,
        stale_pending_comments: delete_stale_pending(db, cfg.guest_ip_retention_days).await?,
        guest_ips_cleared: clear_guest_ips(db, cfg.guest_ip_retention_days).await?,
    })
}

/// 承認されないまま保存期間を過ぎたゲストコメントを消す
async fn delete_stale_pending(db: &DatabaseConnection, days: i64) -> AppResult<u64> {
    if days == 0 {
        return Ok(0);
    }
    Ok(comment::Entity::delete_many()
        .filter(comment::Column::Status.eq("pending"))
        .filter(comment::Column::CreatedAt.lt(Utc::now() - Duration::days(days)))
        .exec(db)
        .await?
        .rows_affected)
}

/// 保存期間を過ぎたゲストコメント・リアクションの IP と User-Agent を消す（コメント・リアクション自体は残す）
async fn clear_guest_ips(db: &DatabaseConnection, days: i64) -> AppResult<u64> {
    if days == 0 {
        return Ok(0);
    }
    let before = Utc::now() - Duration::days(days);
    let c = comment::Entity::update_many()
        .col_expr(comment::Column::Ip, Expr::value(Option::<String>::None))
        .col_expr(comment::Column::UserAgent, Expr::value(Option::<String>::None))
        .filter(comment::Column::CreatedAt.lt(before))
        .filter(
            sea_orm::Condition::any()
                .add(comment::Column::Ip.is_not_null())
                .add(comment::Column::UserAgent.is_not_null()),
        )
        .exec(db)
        .await?
        .rows_affected;
    let r = reaction::Entity::update_many()
        .col_expr(reaction::Column::Ip, Expr::value(Option::<String>::None))
        .filter(reaction::Column::CreatedAt.lt(before))
        .filter(reaction::Column::Ip.is_not_null())
        .exec(db)
        .await?
        .rows_affected;
    Ok(c + r)
}

async fn delete_old_logs(db: &DatabaseConnection, days: i64) -> AppResult<u64> {
    if days == 0 {
        return Ok(0);
    }
    let before = Utc::now() - Duration::days(days);
    let a = access_log::Entity::delete_many()
        .filter(access_log::Column::CreatedAt.lt(before))
        .exec(db)
        .await?
        .rows_affected;
    let l = login_attempt::Entity::delete_many()
        .filter(login_attempt::Column::CreatedAt.lt(before))
        .exec(db)
        .await?
        .rows_affected;
    Ok(a + l)
}

async fn purge_old_trash(db: &DatabaseConnection, days: i64) -> AppResult<u64> {
    if days == 0 {
        return Ok(0);
    }
    let roots: Vec<i32> = page::Entity::find()
        .select_only()
        .column(page::Column::Id)
        .filter(page::Column::DeletedAt.lt(Utc::now() - Duration::days(days)))
        .filter(Expr::col(page::Column::TrashRootId).eq(Expr::col(page::Column::Id)))
        .into_tuple()
        .all(db)
        .await?;
    let mut count = 0;
    for id in roots {
        match trash::purge(db, id).await {
            Ok(n) => count += n,
            // 先に消した削除単位の子孫として既に消えている
            Err(AppError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(count)
}

/// ページごとに新しい方から keep 件だけ残す
async fn prune_revisions(db: &DatabaseConnection, keep: u64) -> AppResult<u64> {
    if keep == 0 {
        return Ok(0);
    }
    let over: Vec<i32> = page_revision::Entity::find()
        .select_only()
        .column(page_revision::Column::PageId)
        .group_by(page_revision::Column::PageId)
        .having(Expr::col(page_revision::Column::Id).count().gt(keep))
        .into_tuple()
        .all(db)
        .await?;
    let mut count = 0;
    for page_id in over {
        // 残す中で一番古い版の id。これより小さいものを消す
        let oldest_kept: Option<i32> = page_revision::Entity::find()
            .select_only()
            .column(page_revision::Column::Id)
            .filter(page_revision::Column::PageId.eq(page_id))
            .order_by_desc(page_revision::Column::Id)
            .offset(keep - 1)
            .limit(1)
            .into_tuple()
            .one(db)
            .await?;
        if let Some(cut) = oldest_kept {
            count += page_revision::Entity::delete_many()
                .filter(page_revision::Column::PageId.eq(page_id))
                .filter(page_revision::Column::Id.lt(cut))
                .exec(db)
                .await?
                .rows_affected;
        }
    }
    Ok(count)
}

/// ページ（ゴミ箱を含む）・履歴・スニペットのどこからも参照されていない画像を消す
async fn remove_unused_uploads(db: &DatabaseConnection, upload_dir: &Path) -> AppResult<u64> {
    let candidates = upload::Entity::find()
        .filter(upload::Column::CreatedAt.lt(Utc::now() - Duration::hours(UPLOAD_GRACE_HOURS)))
        .all(db)
        .await?;
    if candidates.is_empty() {
        return Ok(0);
    }

    // 本文は BlockNote のブロック配列。スニペットも同じ形で、編集ダイアログから画像を貼れる
    let mut used = BTreeSet::new();
    collect_upload_names(db, page::Entity::find(), page::Column::Content, page::Column::Id, &mut used).await?;
    collect_upload_names(db, page_revision::Entity::find(), page_revision::Column::Content, page_revision::Column::Id, &mut used).await?;
    collect_upload_names(db, block_snippet::Entity::find(), block_snippet::Column::Content, block_snippet::Column::Id, &mut used).await?;

    let mut count = 0;
    for u in candidates
        .into_iter()
        .filter(|u| !used.contains(&u.filename))
    {
        // 保存名は自分で付けたものだが、念のためパス区切りを含む名前は触らない
        if u.filename.contains(['/', '\\']) || u.filename.starts_with('.') {
            continue;
        }
        match tokio::fs::remove_file(upload_dir.join(&u.filename)).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!("{} を削除できません: {e}", u.filename);
                continue;
            }
        }
        upload::Entity::delete_by_id(u.id).exec(db).await?;
        count += 1;
    }
    Ok(count)
}

/// 本文の列を少しずつ読み、含まれる `/uploads/<保存名>` を集める
async fn collect_upload_names<E: EntityTrait, C: ColumnTrait>(
    db: &DatabaseConnection,
    find: sea_orm::Select<E>,
    content: C,
    id: C,
    out: &mut BTreeSet<String>,
) -> AppResult<()> {
    let mut rows = find.select_only().column(content).order_by_asc(id).into_tuple::<String>().paginate(db, SCAN_BATCH);
    while let Some(batch) = rows.fetch_and_next().await? {
        batch.iter().for_each(|c| refs::upload_names(c, out));
    }
    Ok(())
}
