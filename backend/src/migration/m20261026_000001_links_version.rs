use sea_orm_migration::{prelude::*, schema::*};

/// バックリンク用のリンク表と、同時編集の衝突検知用の版番号
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    // タイトル・アイコン・本文を保存するたびに1つ増やす
                    .add_column(integer("version").default(1))
                    .to_owned(),
            )
            .await?;

        // 本文中のページリンク（保存時に作り直す）。リンク先は削除済みでも残すので外部キーにしない
        manager
            .create_table(
                Table::create()
                    .table("page_links")
                    .if_not_exists()
                    .col(integer("from_page_id"))
                    .col(integer("to_page_id"))
                    .primary_key(Index::create().col("from_page_id").col("to_page_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("page_links", "from_page_id")
                            .to("pages", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_page_links_to")
                    .table("page_links")
                    .col("to_page_id")
                    .to_owned(),
            )
            .await?;

        // 起動時の backfill でリンク表も埋まるよう、検索用テキストを未生成に戻す
        manager
            .get_connection()
            .execute_unprepared("UPDATE pages SET search_text = NULL")
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("page_links").to_owned())
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("version")
                    .to_owned(),
            )
            .await
    }
}
