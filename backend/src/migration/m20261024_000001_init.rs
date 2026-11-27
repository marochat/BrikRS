use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("users")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    .col(string_len_uniq("username", 64))
                    .col(string_len("display_name", 128))
                    .col(string_len("password_hash", 255))
                    .col(boolean("is_admin").default(false))
                    .col(date_time("created_at"))
                    .to_owned(),
            )
            .await?;

        // セッションIDそのものは保存せず、SHA-256ハッシュだけを持つ
        manager
            .create_table(
                Table::create()
                    .table("sessions")
                    .if_not_exists()
                    .col(char_len("id", 64).primary_key())
                    .col(integer("user_id"))
                    .col(char_len("csrf_token", 64))
                    .col(date_time("expires_at"))
                    .col(date_time("created_at"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("sessions", "user_id")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("pages")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    .col(integer_null("parent_id"))
                    .col(string_len("title", 255).default(""))
                    .col(string_len("icon", 16).default(""))
                    // BlockNote のブロック配列(JSON)をそのまま保存する
                    .col(ColumnDef::new("content").custom("LONGTEXT").not_null())
                    .col(integer("sort_order").default(0))
                    .col(integer_null("created_by"))
                    .col(integer_null("updated_by"))
                    .col(date_time("created_at"))
                    .col(date_time("updated_at"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("pages", "parent_id")
                            .to("pages", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("pages", "created_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("pages", "updated_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_pages_parent_sort")
                    .table("pages")
                    .col("parent_id")
                    .col("sort_order")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for t in ["pages", "sessions", "users"] {
            manager.drop_table(Table::drop().table(t).to_owned()).await?;
        }
        Ok(())
    }
}
