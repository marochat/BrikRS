use sea_orm_migration::{prelude::*, schema::*};

/// 2段階認証（TOTP）
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("users")
                    // Base32 の秘密鍵。NULL なら2段階認証は無効
                    .add_column(string_len_null("totp_secret", 64))
                    // 最後に受け付けたコードの時間区間（同じコードの使い回しを防ぐ）
                    .add_column(big_integer_null("totp_last_step"))
                    // 未使用のリカバリーコードの SHA-256 を JSON 配列で
                    .add_column(text_null("totp_recovery"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("users")
                    .drop_column("totp_secret")
                    .drop_column("totp_last_step")
                    .drop_column("totp_recovery")
                    .to_owned(),
            )
            .await
    }
}
