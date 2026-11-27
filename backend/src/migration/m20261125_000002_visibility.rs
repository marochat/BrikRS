use sea_orm_migration::{prelude::*, schema::*};

/// ページの公開範囲（公開 / 登録ユーザー / グループ）と、ユーザーのグループ
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    // public（誰でも）/ members（ログインしている人）/ groups（page_groups のグループの人と管理者）。
                    // 子孫にも効く。下書き（is_draft）とは別に持ち、下書きを外したときはこの範囲に戻る
                    .add_column(string_len("visibility", 16).default("public"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("user_groups")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    .col(string_len_uniq("name", 64))
                    .col(date_time("created_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("user_group_members")
                    .if_not_exists()
                    .col(integer("group_id"))
                    .col(integer("user_id"))
                    .primary_key(Index::create().col("group_id").col("user_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("user_group_members", "group_id")
                            .to("user_groups", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("user_group_members", "user_id")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_user_group_members_user")
                    .table("user_group_members")
                    .col("user_id")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("page_groups")
                    .if_not_exists()
                    .col(integer("page_id"))
                    .col(integer("group_id"))
                    .primary_key(Index::create().col("page_id").col("group_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("page_groups", "page_id")
                            .to("pages", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("page_groups", "group_id")
                            .to("user_groups", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for t in ["page_groups", "user_group_members", "user_groups"] {
            manager.drop_table(Table::drop().table(t).to_owned()).await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("visibility")
                    .to_owned(),
            )
            .await
    }
}
