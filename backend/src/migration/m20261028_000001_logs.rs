use sea_orm_migration::{prelude::*, schema::*};

/// アクセス解析（ページビュー）とログイン試行の記録
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 画面遷移ごとに1行（ログインしていない閲覧者のみ）
        manager
            .create_table(
                Table::create()
                    .table("access_logs")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(string_len("path", 255))
                    // localStorage / sessionStorage に置いた匿名 ID（ユニーク訪問者数・セッション数の集計用）
                    .col(char_len("visitor_id", 36))
                    .col(char_len("session_id", 36))
                    .col(string_len_null("ip", 64))
                    .col(string_len_null("user_agent", 255))
                    .col(string_len_null("referrer", 255))
                    // User-Agent による簡易な bot 判定（集計から除く）
                    .col(boolean("is_bot").default(false))
                    .col(date_time("created_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_access_logs_created")
                    .table("access_logs")
                    .col("created_at")
                    .to_owned(),
            )
            .await?;

        // ログインの試行（パスワード・2段階認証、成功・失敗、回数制限で止めたもの）
        manager
            .create_table(
                Table::create()
                    .table("login_attempts")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    // 存在しないユーザー名もそのまま残す（狙われた名前を把握するため）
                    .col(string_len("username", 150))
                    .col(string_len_null("ip", 64))
                    .col(string_len_null("user_agent", 255))
                    // password / totp / blocked（回数制限で止めた）
                    .col(string_len("kind", 16))
                    .col(boolean("success"))
                    .col(date_time("created_at"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_login_attempts_created")
                    .table("login_attempts")
                    .col("created_at")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for t in ["login_attempts", "access_logs"] {
            manager
                .drop_table(Table::drop().table(t).to_owned())
                .await?;
        }
        Ok(())
    }
}
