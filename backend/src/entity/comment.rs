use sea_orm::entity::prelude::*;

/// タイムラインの投稿へのコメント（ゲストは承認制、管理者の返信はすぐ公開）
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "comments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub page_id: i32,
    /// 管理者の返信なら返信先のコメント
    pub parent_id: Option<i32>,
    /// 管理者の返信なら、その人
    pub user_id: Option<i32>,
    pub author_name: String,
    #[sea_orm(column_type = "Text")]
    pub body: String,
    /// pending / approved
    pub status: String,
    pub link_count: i16,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub delete_token_hash: Option<String>,
    pub created_at: DateTimeUtc,
    pub approved_at: Option<DateTimeUtc>,
    pub approved_by: Option<i32>,
}

impl ActiveModelBehavior for ActiveModel {}
