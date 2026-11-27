use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
    QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, ExprTrait},
};
use serde::Serialize;
use serde_json::{Value, json};

use super::pages;
use crate::{
    AppState,
    access::{self, Viewer},
    auth::AuthUser,
    entity::page,
    error::{AppError, AppResult},
};

#[derive(Serialize)]
pub struct TrashItem {
    id: i32,
    title: String,
    icon: String,
    /// 一緒にゴミ箱へ入った子孫の数
    descendants: usize,
    deleted_at: Option<DateTime<Utc>>,
    deleted_by: Option<String>,
}

/// viewer に見えないページ（ゴミ箱のものも、削除する前の位置で判定する）
async fn hidden_with_trash(db: &impl ConnectionTrait, v: &Viewer) -> AppResult<HashSet<i32>> {
    if v.is_admin() {
        return Ok(HashSet::new());
    }
    let all = pages::summaries_with_trash(db).await?;
    pages::hidden_in(db, &all, v).await
}

/// 見えない削除単位は 404。完全な削除で一緒に消えるページ（下にある、先にゴミ箱へ入れたものも含む）に
/// 見えないページがあれば断る（管理者は hidden が空なので通る）
async fn check_purge(db: &impl ConnectionTrait, id: i32, hidden: &HashSet<i32>) -> AppResult<()> {
    if hidden.is_empty() {
        return Ok(());
    }
    if hidden.contains(&id) {
        return Err(AppError::NotFound);
    }
    let pairs: Vec<(i32, Option<i32>)> = page::Entity::find()
        .select_only()
        .columns([page::Column::Id, page::Column::ParentId])
        .into_tuple()
        .all(db)
        .await?;
    if pages::subtree_levels(&pairs, id).into_iter().flatten().any(|i| hidden.contains(&i)) {
        return Err(AppError::BadRequest(
            "あなたには見えないページ（他の人の下書きなど）が含まれているため、完全に削除できません".into(),
        ));
    }
    Ok(())
}

pub async fn list(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
) -> AppResult<Json<Vec<TrashItem>>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let hidden = hidden_with_trash(&st.db, &v).await?;
    let trashed = page::Entity::find()
        .filter(page::Column::DeletedAt.is_not_null())
        .order_by_desc(page::Column::DeletedAt)
        .all(&st.db)
        .await?;
    let mut group_size: HashMap<i32, usize> = HashMap::new();
    for p in &trashed {
        if let Some(root) = p.trash_root_id {
            *group_size.entry(root).or_default() += 1;
        }
    }
    let names = pages::user_names(&st.db, trashed.iter().map(|p| p.deleted_by)).await?;
    Ok(Json(
        trashed
            .into_iter()
            .filter(|p| p.trash_root_id == Some(p.id) && !hidden.contains(&p.id))
            .map(|p| TrashItem {
                descendants: group_size.get(&p.id).copied().unwrap_or(1) - 1,
                deleted_by: p.deleted_by.and_then(|i| names.get(&i).cloned()),
                id: p.id,
                title: p.title,
                icon: p.icon,
                deleted_at: p.deleted_at,
            })
            .collect(),
    ))
}

async fn find_trash_root(db: &impl ConnectionTrait, id: i32) -> AppResult<page::Model> {
    page::Entity::find_by_id(id)
        .filter(page::Column::TrashRootId.eq(id))
        .one(db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn restore(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    if hidden_with_trash(&st.db, &v).await?.contains(&id) {
        return Err(AppError::NotFound);
    }
    let txn = st.db.begin().await?;
    let root = find_trash_root(&txn, id).await?;

    // 親がゴミ箱に入ったままなら、最上位に出す
    let parent_alive = match root.parent_id {
        Some(pid) => pages::find_live_page(&txn, pid).await.is_ok(),
        None => true,
    };
    let mut made_draft = false;
    if !parent_alive {
        // 最上位に出すと、祖先の下書き・公開範囲の制限が外れて見える人が増える（下書きの子が公開になるなど）。
        // 制限を受けていたなら、作った人と管理者だけに見える下書きにして戻す（公開するかは作った人が決める）
        let all = pages::summaries_with_trash(&txn).await?;
        made_draft = !root.is_draft && access::restricted_by_ancestors(&all, root.id);
        let last = page::Entity::find()
            .filter(page::Column::ParentId.is_null())
            .order_by_desc(page::Column::SortOrder)
            .one(&txn)
            .await?;
        let mut am = page::ActiveModel {
            id: Set(root.id),
            parent_id: Set(None),
            sort_order: Set(last.map_or(0, |l| l.sort_order + 1)),
            ..Default::default()
        };
        if made_draft {
            am.is_draft = Set(true);
        }
        am.update(&txn).await?;
    }

    page::Entity::update_many()
        .col_expr(page::Column::DeletedAt, Expr::value(Option::<DateTime<Utc>>::None))
        .col_expr(page::Column::DeletedBy, Expr::value(Option::<i32>::None))
        .col_expr(page::Column::TrashRootId, Expr::value(Option::<i32>::None))
        .filter(page::Column::TrashRootId.eq(id))
        .exec(&txn)
        .await?;
    txn.commit().await?;
    Ok(Json(json!({
        "pages": pages::visible_summaries(&st.db, &v).await?,
        "made_draft": made_draft,
    })))
}

/// ゴミ箱のページを子孫ごと完全に削除する
pub async fn purge(db: &DatabaseConnection, id: i32) -> AppResult<u64> {
    let txn = db.begin().await?;
    find_trash_root(&txn, id).await?;
    let pairs: Vec<(i32, Option<i32>)> = page::Entity::find()
        .select_only()
        .columns([page::Column::Id, page::Column::ParentId])
        .into_tuple()
        .all(&txn)
        .await?;
    // InnoDB の CASCADE は入れ子15段までしか辿らないので、深い方から消す
    let mut count = 0;
    for ids in pages::subtree_levels(&pairs, id).into_iter().rev() {
        count += page::Entity::delete_many()
            .filter(page::Column::Id.is_in(ids))
            .exec(&txn)
            .await?
            .rows_affected;
    }
    txn.commit().await?;
    Ok(count)
}

pub async fn delete(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Value>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    find_trash_root(&st.db, id).await?;
    check_purge(&st.db, id, &hidden_with_trash(&st.db, &v).await?).await?;
    let count = purge(&st.db, id).await?;
    Ok(Json(json!({ "deleted": count })))
}

/// ゴミ箱を空にする。管理者以外は、自分に見える削除単位（見えないページを含まないもの）だけ
pub async fn empty(State(st): State<AppState>, AuthUser(cu): AuthUser) -> AppResult<Json<Value>> {
    let v = access::viewer(&st.db, Some(&cu)).await?;
    let hidden = hidden_with_trash(&st.db, &v).await?;
    let roots: Vec<i32> = page::Entity::find()
        .select_only()
        .column(page::Column::Id)
        .filter(page::Column::DeletedAt.is_not_null())
        .filter(Expr::col(page::Column::TrashRootId).eq(Expr::col(page::Column::Id)))
        .into_tuple()
        .all(&st.db)
        .await?;
    let mut count = 0;
    for id in roots {
        if check_purge(&st.db, id, &hidden).await.is_err() {
            continue;
        }
        // 先に消した削除単位の子孫として既に消えている場合がある
        match purge(&st.db, id).await {
            Ok(n) => count += n,
            Err(AppError::NotFound) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(Json(json!({ "deleted": count })))
}
