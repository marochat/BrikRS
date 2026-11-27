use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "page_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub from_page_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub to_page_id: i32,
}

impl ActiveModelBehavior for ActiveModel {}
