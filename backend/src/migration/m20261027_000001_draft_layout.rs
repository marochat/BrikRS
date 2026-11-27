use sea_orm_migration::{prelude::*, schema::*};

/// 下書き（非公開）と、ページの表示形式（通常 / タイムライン）
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    // true なら編集者にだけ見える（子孫も含めて）
                    .add_column(boolean("is_draft").default(false))
                    // page（通常）/ timeline（子ページを投稿として新しい順に並べる）
                    .add_column(string_len("layout", 16).default("page"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_pages_updated")
                    .table("pages")
                    .col("updated_at")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_pages_updated")
                    .table("pages")
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("is_draft")
                    .drop_column("layout")
                    .to_owned(),
            )
            .await
    }
}
