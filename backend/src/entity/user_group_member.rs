use sea_orm::entity::prelude::*;

/// グループに所属するユーザー（1人が複数のグループに入れる）
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "user_group_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i32,
}

impl ActiveModelBehavior for ActiveModel {}
