use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "login_attempts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub username: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// password / totp / blocked
    pub kind: String,
    pub success: bool,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
