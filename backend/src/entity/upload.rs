use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "uploads")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 保存名（ランダム + 拡張子）
    #[sea_orm(unique)]
    pub filename: String,
    pub original_name: String,
    pub mime: String,
    pub size: i64,
    pub uploaded_by: Option<i32>,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
