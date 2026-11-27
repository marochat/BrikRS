use sea_orm::entity::prelude::*;

/// タイムラインの投稿・返信へのリアクション。同じ訪問者・同じ種類は1回だけ
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "reactions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub page_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub kind: String,
    /// 訪問者 Cookie の値の SHA-256
    #[sea_orm(primary_key, auto_increment = false)]
    pub visitor_hash: String,
    pub ip: Option<String>,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
