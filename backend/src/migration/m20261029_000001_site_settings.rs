use sea_orm_migration::{prelude::*, schema::*};

/// サイト全体の設定（タイトルロゴなど）。1行が1項目
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("site_settings")
                    .if_not_exists()
                    .col(string_len("name", 64).primary_key())
                    .col(text("value"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("site_settings").to_owned())
            .await
    }
}
