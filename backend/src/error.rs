use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized,
    Forbidden,
    NotFound,
    TooManyRequests,
    /// 同時編集の衝突。最新の状態を一緒に返す
    Conflict(serde_json::Value),
    Internal(anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "ログインが必要です".into()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "権限がありません".into()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "見つかりません".into()),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "試行回数が多すぎます。しばらく待ってから再度お試しください".into(),
            ),
            AppError::Conflict(latest) => {
                let body = json!({
                    "error": "他の人がこのページを更新しました",
                    "latest": latest,
                });
                return (StatusCode::CONFLICT, Json(body)).into_response();
            }
            AppError::Internal(e) => {
                // 内部エラーの詳細はログにだけ出し、クライアントには返さない
                tracing::error!("internal error: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "サーバーエラー".into())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(e: sea_orm::DbErr) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}
