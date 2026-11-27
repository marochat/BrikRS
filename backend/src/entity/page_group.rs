use sea_orm::entity::prelude::*;

/// 公開範囲が groups のページを見られるグループ
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "page_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub page_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: i32,
}

impl ActiveModelBehavior for ActiveModel {}
