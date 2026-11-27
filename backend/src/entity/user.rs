use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub created_at: DateTimeUtc,
    /// 2段階認証の秘密鍵（Base32）。None なら無効
    pub totp_secret: Option<String>,
    pub totp_last_step: Option<i64>,
    /// 未使用のリカバリーコードの SHA-256（JSON 配列）
    #[sea_orm(column_type = "Text", nullable)]
    pub totp_recovery: Option<String>,
    /// カラーテーマ（auto / light / dark）。None は auto
    pub theme: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
