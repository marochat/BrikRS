//! 画像アップロード。
//!
//! 拡張子や Content-Type は信用せず、ファイル先頭のバイト列で形式を判定する。
//! SVG はスクリプトを含められるので受け付けない。

use axum::{
    Json,
    extract::{Multipart, State},
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};

use crate::{
    AppState,
    auth::{self, AuthUser},
    entity::upload,
    error::{AppError, AppResult},
};

pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

pub fn detect_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("png", "image/png"))
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(("jpg", "image/jpeg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("gif", "image/gif"))
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("webp", "image/webp"))
    } else {
        None
    }
}

pub async fn upload(
    State(st): State<AppState>,
    AuthUser(cu): AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    let bad = |m: &str| AppError::BadRequest(m.into());
    let field = loop {
        let f = multipart
            .next_field()
            .await
            .map_err(|_| bad("アップロードの形式が不正です"))?
            .ok_or_else(|| bad("ファイルがありません"))?;
        if f.name() == Some("file") {
            break f;
        }
    };
    let original_name: String = field
        .file_name()
        .unwrap_or("image")
        .chars()
        .filter(|c| !c.is_control())
        .take(255)
        .collect();
    let bytes = field
        .bytes()
        .await
        .map_err(|_| bad("ファイルが大きすぎるか、読み込めませんでした（上限10MB）"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(bad("ファイルが大きすぎます（上限10MB）"));
    }
    let (ext, mime) =
        detect_image(&bytes).ok_or_else(|| bad("PNG / JPEG / GIF / WebP の画像のみ対応しています"))?;

    let url = save_image(&st, &bytes, ext, mime, original_name, cu.user.id).await?;
    Ok(Json(json!({ "url": url })))
}

/// 判定済みの画像を保存して記録し、`/uploads/<保存名>` を返す（Web ブックマークの画像もここに置く）
pub async fn save_image(
    st: &AppState,
    bytes: &[u8],
    ext: &str,
    mime: &str,
    original_name: String,
    user_id: i32,
) -> AppResult<String> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::BadRequest("ファイルが大きすぎます".into()));
    }
    // 推測されにくいランダムな名前で保存する（128bit）
    let filename = format!("{}.{ext}", &auth::random_token()[..32]);
    let path = st.upload_dir.join(&filename);
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    upload::ActiveModel {
        filename: Set(filename.clone()),
        original_name: Set(original_name),
        mime: Set(mime.into()),
        size: Set(bytes.len() as i64),
        uploaded_by: Set(Some(user_id)),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&st.db)
    .await?;
    Ok(format!("/uploads/{filename}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_magic_bytes() {
        assert_eq!(detect_image(b"\x89PNG\r\n\x1a\n....").unwrap().0, "png");
        assert_eq!(detect_image(b"RIFF\0\0\0\0WEBPVP8 ").unwrap().0, "webp");
        assert!(detect_image(b"<svg xmlns=...").is_none());
        assert!(detect_image(b"<html><script>").is_none());
    }
}
