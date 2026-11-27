use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "access_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub path: String,
    pub visitor_id: String,
    pub session_id: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub referrer: Option<String>,
    pub is_bot: bool,
    pub created_at: DateTimeUtc,
}

impl ActiveModelBehavior for ActiveModel {}
