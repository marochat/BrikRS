use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    /// セッションIDの SHA-256 (hex)
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub user_id: i32,
    pub csrf_token: String,
    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
