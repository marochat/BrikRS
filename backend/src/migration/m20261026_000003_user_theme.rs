use sea_orm_migration::{prelude::*, schema::*};

/// ユーザーごとのカラーテーマ
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("users")
                    // auto（OS の設定に従う）/ light / dark。NULL は auto と同じ
                    .add_column(string_len_null("theme", 16))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("users")
                    .drop_column("theme")
                    .to_owned(),
            )
            .await
    }
}
