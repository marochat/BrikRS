use sea_orm_migration::{prelude::*, schema::*};

/// ブロックスニペット（「/」メニューから挿入する、よく使う複数ブロックの組み合わせ）
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("block_snippets")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    .col(string_len("name", 64))
                    .col(string_len("icon", 16).default(""))
                    // BlockNote のブロック配列(JSON)をそのまま保存する
                    .col(ColumnDef::new("content").custom("LONGTEXT").not_null())
                    .col(integer("sort_order").default(0))
                    .col(integer_null("created_by"))
                    .col(date_time("created_at"))
                    .col(date_time("updated_at"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("block_snippets", "created_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("block_snippets").to_owned())
            .await
    }
}
