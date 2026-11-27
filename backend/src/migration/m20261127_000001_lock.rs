use sea_orm_migration::{prelude::*, schema::*};

/// ページのロック（作った人と管理者以外は閲覧だけ。ロック中は誰も移動・ゴミ箱にできない）
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .add_column(boolean("is_locked").default(false))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("is_locked")
                    .to_owned(),
            )
            .await
    }
}
