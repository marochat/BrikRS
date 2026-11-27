use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, put},
};
use serde::Serialize;

use crate::{AppState, entity::user};

mod bookmark;
mod comments;
mod gist;
mod groups;
pub(crate) mod interactions;
pub(crate) mod pages;
mod revisions;
mod search;
pub(crate) mod monitor;
mod session;
pub(crate) mod site;
mod snippets;
mod tweet;
pub(crate) mod totp;
pub(crate) mod trash;
pub(crate) mod uploads;
mod users;

pub use pages::backfill_index;
pub use users::{create_user, find_user};

/// 2段階認証の解除と、そのユーザーの全セッションの削除（CLI 用）
pub async fn clear_totp(db: &sea_orm::DatabaseConnection, user_id: i32) -> crate::error::AppResult<()> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    totp::clear_totp(db, user_id).await?;
    crate::entity::session::Entity::delete_many()
        .filter(crate::entity::session::Column::UserId.eq(user_id))
        .exec(db)
        .await?;
    Ok(())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(session::me))
        .route("/me/password", post(session::change_password))
        .route("/me/theme", put(session::set_theme))
        .route("/login", post(session::login))
        .route("/login/totp", post(session::login_totp))
        .route("/me/totp", get(totp::status))
        .route("/me/totp/setup", post(totp::setup))
        .route("/me/totp/enable", post(totp::enable))
        .route("/me/totp/disable", post(totp::disable))
        .route("/me/totp/recovery-codes", post(totp::regenerate_recovery))
        .route("/logout", post(session::logout))
        .route("/pages", get(pages::list).post(pages::create))
        .route(
            "/pages/{id}",
            get(pages::get_one).put(pages::update).delete(pages::delete),
        )
        .route("/pages/{id}/move", put(pages::move_page))
        .route("/pages/{id}/backlinks", get(pages::backlinks))
        .route("/pages/{id}/settings", put(pages::update_settings))
        .route("/pages/{id}/posts", get(pages::posts))
        .route("/pages/{id}/archive", get(pages::archive))
        .route("/pages/{id}/thread", get(pages::thread))
        .route("/pages/{id}/rows", get(pages::rows))
        .route("/pages/{id}/comments", get(comments::list).post(comments::create))
        .route(
            "/pages/{id}/reactions/{kind}",
            put(interactions::add).delete(interactions::remove),
        )
        .route("/comment-token", get(comments::form_token))
        .route("/comments/mine", post(comments::mine))
        .route("/comments/{id}", axum::routing::delete(comments::admin_delete))
        .route("/comments/{id}/mine", axum::routing::delete(comments::delete_mine))
        .route("/comments/{id}/approve", post(comments::approve))
        .route("/comments/{id}/reply", post(comments::reply))
        .route("/admin/comments", get(comments::admin_list))
        .route("/admin/comments/count", get(comments::admin_count))
        .route("/admin/comments/bulk", post(comments::bulk))
        .route("/recent", get(pages::recent))
        .route("/pageview", post(monitor::pageview))
        .route("/admin/access", get(monitor::access_summary))
        .route("/admin/logins", get(monitor::login_summary))
        .route("/site", get(site::get).put(site::update))
        .route("/pages/{id}/revisions", get(revisions::list))
        .route("/revisions/{id}", get(revisions::get_one))
        .route("/revisions/{id}/restore", post(revisions::restore))
        .route("/search", get(search::search))
        .route("/bookmark", post(bookmark::create))
        .route("/gist", post(gist::create))
        .route("/tweet", post(tweet::create))
        .route("/snippets", get(snippets::list).post(snippets::create))
        .route("/snippets/{id}", put(snippets::update).delete(snippets::delete))
        .route("/trash", get(trash::list).delete(trash::empty))
        .route("/trash/{id}", axum::routing::delete(trash::delete))
        .route("/trash/{id}/restore", post(trash::restore))
        .route(
            "/uploads",
            // multipart の区切り分を見込んで上限より少し大きくする
            post(uploads::upload).layer(DefaultBodyLimit::max(uploads::MAX_UPLOAD_BYTES + 64 * 1024)),
        )
        .route("/users", get(users::list).post(users::create))
        .route("/users/{id}", put(users::update).delete(users::delete))
        .route("/groups", get(groups::list).post(groups::create))
        .route("/groups/{id}", put(groups::update).delete(groups::delete))
}

#[derive(Serialize)]
pub struct PublicUser {
    pub id: i32,
    pub username: String,
    pub display_name: String,
    pub is_admin: bool,
    pub totp_enabled: bool,
    pub theme: String,
}

impl From<user::Model> for PublicUser {
    fn from(u: user::Model) -> Self {
        Self {
            id: u.id,
            username: u.username,
            display_name: u.display_name,
            is_admin: u.is_admin,
            totp_enabled: u.totp_secret.is_some(),
            theme: u.theme.unwrap_or_else(|| "auto".into()),
        }
    }
}
