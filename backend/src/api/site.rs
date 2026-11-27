//! サイト全体の設定（タイトルロゴと、その下の小さな一文、ゲストコメント・リアクションの受け付け）。
//! 起動時に DB から読んで AppState に持ち、管理画面で変えたらそこも書き換える（HTML を返すたびに DB を引かない）
//!
//! site_settings 表は (name, value) の汎用 KV。name は SiteSettings のフィールド名そのもので、読み書きは
//! serde 経由で行う。設定を足すときは、フィールドと既定値（Default）と update の検査を足すだけでよい
//! （マイグレーション不要。フィールドはすべて String にする）

use std::sync::{Arc, RwLock};

use axum::{Json, extract::State};
use sea_orm::{ConnectionTrait, EntityTrait, Set, sea_query::OnConflict};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    AppState,
    auth::AdminUser,
    entity::site_setting,
    error::{AppError, AppResult},
};

const TITLE_MAX_CHARS: usize = 60;
const TAGLINE_MAX_CHARS: usize = 100;
/// ロゴの下の一文：無し / 「BrikRS v0.x」（値は powered のまま） / 自由に書いた一文
const TAGLINES: &[&str] = &["none", "powered", "custom"];
const SWITCHES: &[&str] = &["off", "on"];
/// アップロード前に縮小する画像の長い辺の上限（px）。0 は縮小しない
const IMAGE_MAX_PX_RANGE: std::ops::RangeInclusive<u32> = 320..=10000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteSettings {
    pub title: String,
    pub tagline: String,
    pub tagline_text: String,
    /// タイムラインの投稿へのゲストコメント（承認制）。off / on。on でも、ページごとに受け付けたものだけ
    pub guest_comments: String,
    /// タイムラインの投稿・返信へのリアクション。off / on。on でも、ページごとに受け付けたものだけ
    pub reactions: String,
    /// ゲストコメントに画像の CAPTCHA を付けるか。off / on
    pub comment_captcha: String,
    /// アップロードする画像の長い辺の上限（px、"0" は縮小しない）。縮小はブラウザで行う（転送量を減らすため）
    pub image_max_px: String,
}

pub type SharedSite = Arc<RwLock<SiteSettings>>;

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            // 0.3 までは環境変数 SITE_NAME（フィードと共有カード用）だった。管理画面で変えるまではそれを使う
            title: std::env::var("SITE_NAME")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "BrikRS".into()),
            tagline: "none".into(),
            tagline_text: String::new(),
            guest_comments: "off".into(),
            reactions: "off".into(),
            comment_captcha: "on".into(),
            image_max_px: "0".into(),
        }
    }
}

impl SiteSettings {
    pub fn guest_comments_on(&self) -> bool {
        self.guest_comments == "on"
    }

    pub fn reactions_on(&self) -> bool {
        self.reactions == "on"
    }

    pub fn comment_captcha_on(&self) -> bool {
        self.comment_captcha == "on"
    }

    /// フィールド名 → 値。DB の行と1対1
    fn to_map(&self) -> Map<String, Value> {
        match serde_json::to_value(self) {
            Ok(Value::Object(m)) => m,
            _ => unreachable!("SiteSettings は文字列フィールドだけの構造体"),
        }
    }

    /// 既定値に DB の行を重ねる。知らない name（古い版の設定など）は無視する
    fn from_rows(rows: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut map = Self::default().to_map();
        for (name, value) in rows {
            if map.contains_key(&name) {
                map.insert(name, Value::String(value));
            }
        }
        serde_json::from_value(Value::Object(map)).unwrap_or_default()
    }

    pub async fn load(db: &impl ConnectionTrait) -> AppResult<Self> {
        let rows = site_setting::Entity::find().all(db).await?;
        Ok(Self::from_rows(rows.into_iter().map(|r| (r.name, r.value))))
    }
}

/// 今のサイトのタイトル（フィード・共有カード・HTML の <title> 用）
pub fn site_title(st: &AppState) -> String {
    st.site.read().unwrap_or_else(|e| e.into_inner()).title.clone()
}

/// 今のサイト設定の写し
pub fn current(st: &AppState) -> SiteSettings {
    st.site.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// ログインしていない人にも返す（ナビバーのロゴに使う）
pub async fn get(State(st): State<AppState>) -> Json<SiteSettings> {
    Json(st.site.read().unwrap_or_else(|e| e.into_inner()).clone())
}

pub async fn update(
    State(st): State<AppState>,
    AdminUser(_): AdminUser,
    Json(req): Json<SiteSettings>,
) -> AppResult<Json<SiteSettings>> {
    let s = SiteSettings {
        title: req.title.trim().to_string(),
        tagline: req.tagline,
        tagline_text: req.tagline_text.trim().to_string(),
        guest_comments: req.guest_comments,
        reactions: req.reactions,
        comment_captcha: req.comment_captcha,
        image_max_px: req.image_max_px.trim().to_string(),
    };
    match s.image_max_px.parse::<u32>() {
        Ok(0) => {}
        Ok(n) if IMAGE_MAX_PX_RANGE.contains(&n) => {}
        _ => {
            return Err(AppError::BadRequest(format!(
                "画像の縮小の大きさは 0（縮小しない）か {}〜{} px にしてください",
                IMAGE_MAX_PX_RANGE.start(),
                IMAGE_MAX_PX_RANGE.end()
            )));
        }
    }
    for v in [&s.guest_comments, &s.reactions, &s.comment_captcha] {
        if !SWITCHES.contains(&v.as_str()) {
            return Err(AppError::BadRequest("不明な設定値です".into()));
        }
    }
    if s.title.is_empty() {
        return Err(AppError::BadRequest("タイトルを入力してください".into()));
    }
    if s.title.chars().count() > TITLE_MAX_CHARS {
        return Err(AppError::BadRequest(format!("タイトルは {TITLE_MAX_CHARS} 文字までです")));
    }
    if !TAGLINES.contains(&s.tagline.as_str()) {
        return Err(AppError::BadRequest("不明な表示方法です".into()));
    }
    if s.tagline_text.chars().count() > TAGLINE_MAX_CHARS {
        return Err(AppError::BadRequest(format!("一文は {TAGLINE_MAX_CHARS} 文字までです")));
    }
    if s.tagline == "custom" && s.tagline_text.is_empty() {
        return Err(AppError::BadRequest("ロゴの下に出す一文を入力してください".into()));
    }

    let rows = s.to_map().into_iter().map(|(name, value)| site_setting::ActiveModel {
        name: Set(name),
        value: Set(value.as_str().unwrap_or_default().to_string()),
    });
    site_setting::Entity::insert_many(rows)
        .on_conflict(
            OnConflict::column(site_setting::Column::Name)
                .update_column(site_setting::Column::Value)
                .to_owned(),
        )
        .exec(&st.db)
        .await?;

    *st.site.write().unwrap_or_else(|e| e.into_inner()) = s.clone();
    Ok(Json(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_overlay_defaults() {
        let rows = [("tagline", "custom"), ("tagline_text", "こんにちは"), ("unknown", "x")]
            .map(|(n, v)| (n.to_string(), v.to_string()));
        let s = SiteSettings::from_rows(rows);
        assert_eq!(s.tagline, "custom");
        assert_eq!(s.tagline_text, "こんにちは");
        // DB に無い設定は既定値のまま
        assert_eq!(s.title, SiteSettings::default().title);
        // 書き込む行は全フィールドぶん
        let names: Vec<_> = s.to_map().keys().cloned().collect();
        assert_eq!(
            names,
            ["comment_captcha", "guest_comments", "image_max_px", "reactions", "tagline", "tagline_text", "title"]
        );
        assert!(!s.guest_comments_on() && !s.reactions_on() && s.comment_captcha_on());
    }
}
