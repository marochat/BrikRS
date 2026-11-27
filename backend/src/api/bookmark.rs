//! Web ブックマーク：URL から OGP を取ってきて、カードに出す内容を返す（取得は ogp.rs）。
//! 画像はこのサーバーに取り込む（元のサイトで消えても残り、閲覧者が外部へ接続しない）

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
    ogp::{self, FetchError},
};

use super::uploads;

#[derive(Deserialize)]
pub struct BookmarkReq {
    url: String,
}

#[derive(Serialize)]
pub struct Bookmark {
    /// 入力された URL（リダイレクト後ではなく、書いた人が選んだもの）
    url: String,
    title: String,
    description: String,
    site_name: String,
    /// 取り込んだ画像（`/uploads/...`）。無ければ空
    image: String,
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    Json(req): Json<BookmarkReq>,
) -> AppResult<Json<Bookmark>> {
    let to_err = |e: FetchError| match e {
        FetchError::Rejected(m) => AppError::BadRequest(m.into()),
        FetchError::Network(m) => AppError::BadRequest(format!("ページを読み込めませんでした：{m}")),
    };
    let url = ogp::parse_url(&req.url).map_err(to_err)?;
    let (final_url, og) = ogp::fetch(url.clone()).await.map_err(to_err)?;

    // 画像は取り込めなくてもカードは作る（画像なしの形で出す）
    let mut image = String::new();
    if let Some(img_url) = og.image {
        match ogp::fetch_image(img_url.clone()).await {
            Ok(bytes) => match uploads::detect_image(&bytes) {
                Some((ext, mime)) => {
                    let name = format!("ogp-{}", final_url.host_str().unwrap_or("image"));
                    image = uploads::save_image(&st, &bytes, ext, mime, name, cu.user.id).await?;
                }
                None => tracing::info!("OGP 画像の形式に未対応: {img_url}"),
            },
            Err(e) => tracing::info!("OGP 画像を取得できません: {img_url}: {e:?}"),
        }
    }

    Ok(Json(Bookmark {
        title: if og.title.is_empty() {
            final_url.host_str().unwrap_or_default().to_string()
        } else {
            og.title
        },
        description: og.description,
        site_name: if og.site_name.is_empty() {
            final_url.host_str().unwrap_or_default().to_string()
        } else {
            og.site_name
        },
        url: url.to_string(),
        image,
    }))
}
