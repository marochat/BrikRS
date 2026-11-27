use sea_orm_migration::{prelude::*, schema::*};

/// ゲストコメント（承認制）とリアクション。どちらもタイムラインの投稿に付ける
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // タイムラインのページごとの受け付け（表示形式が timeline のときだけ意味を持つ）
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .add_column(boolean("allow_guest_comments").default(false))
                    .add_column(boolean("allow_reactions").default(false))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("comments")
                    .if_not_exists()
                    .col(pk_auto("id"))
                    // コメントを付けた投稿（タイムラインの直下のページ）
                    .col(integer("page_id"))
                    // 管理者の返信なら返信先のコメント（返信は1段まで）
                    .col(integer_null("parent_id"))
                    // 管理者の返信なら、その人
                    .col(integer_null("user_id"))
                    .col(string_len("author_name", 64))
                    .col(text("body"))
                    // pending（承認待ち）/ approved（公開）。却下はその場で削除する
                    .col(string_len("status", 16))
                    .col(small_integer("link_count").default(0))
                    // IP・UA は保存期間（GUEST_IP_RETENTION_DAYS）を過ぎたら NULL にする
                    .col(string_len_null("ip", 64))
                    .col(string_len_null("user_agent", 255))
                    // 投稿した人が自分で消すためのトークンの SHA-256
                    .col(char_len_null("delete_token_hash", 64))
                    .col(date_time("created_at"))
                    .col(date_time_null("approved_at"))
                    .col(integer_null("approved_by"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("comments", "page_id")
                            .to("pages", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("comments", "parent_id")
                            .to("comments", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("comments", "user_id")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("comments", "approved_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, cols) in [
            ("idx_comments_page_status", &["page_id", "status", "id"][..]),
            ("idx_comments_status", &["status", "id"][..]),
            ("idx_comments_ip", &["ip", "created_at"][..]),
        ] {
            let mut idx = Index::create();
            idx.name(name).table("comments");
            for c in cols {
                idx.col(*c);
            }
            manager.create_index(idx.to_owned()).await?;
        }

        manager
            .create_table(
                Table::create()
                    .table("reactions")
                    .if_not_exists()
                    .col(integer("page_id"))
                    // like / heart / laugh / wow / party
                    .col(string_len("kind", 16))
                    // 訪問者 Cookie（brikrs_vid）の値の SHA-256
                    .col(char_len("visitor_hash", 64))
                    .col(string_len_null("ip", 64))
                    .col(date_time("created_at"))
                    .primary_key(
                        Index::create()
                            .col("page_id")
                            .col("kind")
                            .col("visitor_hash"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("reactions", "page_id")
                            .to("pages", "id")
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_reactions_page_ip")
                    .table("reactions")
                    .col("page_id")
                    .col("ip")
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_reactions_created")
                    .table("reactions")
                    .col("created_at")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for t in ["reactions", "comments"] {
            manager.drop_table(Table::drop().table(t).to_owned()).await?;
        }
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("allow_guest_comments")
                    .drop_column("allow_reactions")
                    .to_owned(),
            )
            .await
    }
}
