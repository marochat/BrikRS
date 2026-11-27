use sea_orm_migration::{prelude::*, schema::*};

/// プロトタイプ2：ゴミ箱、検索用テキスト、編集履歴、画像アップロード
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    // 本文から抜き出したプレーンテキスト（検索用）。NULL は未生成で、起動時に埋める
                    .add_column(ColumnDef::new("search_text").custom("LONGTEXT").null())
                    .add_column(date_time_null("deleted_at"))
                    .add_column(integer_null("deleted_by"))
                    // ゴミ箱へ入れる操作の起点になったページの id（子孫も同じ値を持つ）
                    .add_column(integer_null("trash_root_id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_pages_deleted_by")
                    .from("pages", "deleted_by")
                    .to("users", "id")
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_pages_trash_root")
                    .table("pages")
                    .col("trash_root_id")
                    .to_owned(),
            )
            .await?;

        // 1行 = 1人の一続きの編集（15分以内の連続した自動保存はまとめる）
        manager
            .create_table(
                Table::create()
                    .table("page_revisions")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    .col(integer("page_id"))
                    .col(string_len("title", 255))
                    .col(string_len("icon", 16))
                    .col(ColumnDef::new("content").custom("LONGTEXT").not_null())
                    .col(integer_null("created_by"))
                    .col(date_time("started_at"))
                    .col(date_time("updated_at"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("page_revisions", "page_id")
                            .to("pages", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("page_revisions", "created_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_revisions_page")
                    .table("page_revisions")
                    .col("page_id")
                    .col("id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("uploads")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    .col(string_len_uniq("filename", 64))
                    .col(string_len("original_name", 255))
                    .col(string_len("mime", 64))
                    .col(big_integer("size"))
                    .col(integer_null("uploaded_by"))
                    .col(date_time("created_at"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("uploads", "uploaded_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table("uploads").to_owned()).await?;
        manager
            .drop_table(Table::drop().table("page_revisions").to_owned())
            .await?;
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name("fk_pages_deleted_by")
                    .table("pages")
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("search_text")
                    .drop_column("deleted_at")
                    .drop_column("deleted_by")
                    .drop_column("trash_root_id")
                    .to_owned(),
            )
            .await
    }
}
