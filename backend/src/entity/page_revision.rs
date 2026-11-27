use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "page_revisions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub page_id: i32,
    pub title: String,
    pub icon: String,
    #[sea_orm(column_type = "Text")]
    pub content: String,
    pub created_by: Option<i32>,
    /// この版にまとめた編集の開始時刻
    pub started_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
