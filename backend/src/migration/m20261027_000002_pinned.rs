use sea_orm_migration::{prelude::*, schema::*};

/// タイムラインの投稿のピン留め（先頭に固定）
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .add_column(boolean("is_pinned").default(false))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("is_pinned")
                    .to_owned(),
            )
            .await
    }
}
