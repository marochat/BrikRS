use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "pages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub parent_id: Option<i32>,
    pub title: String,
    pub icon: String,
    /// BlockNote のブロック配列を JSON 文字列で保持
    #[sea_orm(column_type = "Text")]
    pub content: String,
    pub sort_order: i32,
    pub created_by: Option<i32>,
    pub updated_by: Option<i32>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    /// 検索用のプレーンテキスト（content から生成）
    #[sea_orm(column_type = "Text", nullable)]
    pub search_text: Option<String>,
    /// ゴミ箱に入っていれば削除日時
    pub deleted_at: Option<DateTimeUtc>,
    pub deleted_by: Option<i32>,
    pub trash_root_id: Option<i32>,
    /// 保存のたびに増える版番号（同時編集の衝突検知用）
    pub version: i32,
    /// 下書きなら作った人と管理者にだけ見える（子孫も含めて）
    pub is_draft: bool,
    /// 公開範囲（public / members / groups）。子孫にも効く。下書きを外したときはこの範囲になる
    pub visibility: String,
    /// page（通常）/ timeline（子ページを投稿として並べる）
    pub layout: String,
    /// タイムラインの先頭に固定する投稿
    pub is_pinned: bool,
    /// データベースページ（layout = "database"）が持つ列定義。JSON文字列
    #[sea_orm(column_type = "Text", nullable)]
    pub db_schema: Option<String>,
    /// 行（データベースページの子ページ）が持つ列の値。JSON文字列
    #[sea_orm(column_type = "Text", nullable)]
    pub properties: Option<String>,
    /// テンプレートとして使えるページの印
    pub is_template: bool,
    /// タイムラインのページで、投稿へのゲストコメントを受け付けるか（承認制）
    pub allow_guest_comments: bool,
    /// タイムラインのページで、投稿・返信へのリアクションを受け付けるか
    pub allow_reactions: bool,
    /// ロック：作った人と管理者以外は閲覧だけ。ロック中は作った人・管理者も移動・ゴミ箱にできない（外してから）。子ページには効かない
    pub is_locked: bool,
}

impl ActiveModelBehavior for ActiveModel {}
