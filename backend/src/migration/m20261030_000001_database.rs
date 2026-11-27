use sea_orm_migration::{prelude::*, schema::*};

/// データベースページ（Notion のデータベースのような表）と、ページテンプレート
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    // データベースページ自身が持つ列定義（JSON配列）。layout = "database" のときのみ意味を持つ
                    .add_column(ColumnDef::new("db_schema").custom("LONGTEXT").null())
                    // 行（データベースページの子ページ）が持つ列の値（JSONオブジェクト）
                    .add_column(ColumnDef::new("properties").custom("LONGTEXT").null())
                    // テンプレートとして使えるページの印（子ページ作成時に内容をコピーする元になる）
                    .add_column(boolean("is_template").default(false))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("pages")
                    .drop_column("db_schema")
                    .drop_column("properties")
                    .drop_column("is_template")
                    .to_owned(),
            )
            .await
    }
}
