//! ページ検索。
//!
//! MariaDB の FULLTEXT は日本語の分かち書きができない（ngram パーサーは MySQL 専用）ため、
//! 保存時に作った search_text に対する LIKE 検索にしている。個人・身内用の規模なら十分速い。

use axum::{
    Json,
    extract::{Query, State},
};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect,
};
use serde::{Deserialize, Serialize};

use super::pages;
use crate::{AppState, auth::MaybeUser, entity::page, error::AppResult};

const MAX_TERMS: usize = 5;
const MAX_RESULTS: u64 = 50;
const SNIPPET_CHARS: usize = 40;

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
}

#[derive(FromQueryResult)]
struct Row {
    id: i32,
    title: String,
    icon: String,
    search_text: Option<String>,
}

#[derive(Serialize)]
pub struct Hit {
    id: i32,
    title: String,
    icon: String,
    snippet: String,
}

/// LIKE の特殊文字を無効化する（MariaDB の既定のエスケープ文字は \）
fn like_pattern(term: &str) -> String {
    let mut s = String::with_capacity(term.len() + 2);
    s.push('%');
    for c in term.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

/// 最初に見つかった語の前後を切り出す
fn snippet(text: &str, terms: &[String]) -> String {
    let lower = text.to_lowercase();
    let chars: Vec<char> = text.chars().collect();
    let hit = terms
        .iter()
        .filter_map(|t| lower.find(&t.to_lowercase()))
        .min()
        // to_lowercase で長さが変わる文字は稀なので、バイト位置→文字位置の変換で十分
        .map(|byte_pos| lower[..byte_pos].chars().count())
        .unwrap_or(0);
    let start = hit.saturating_sub(SNIPPET_CHARS / 2);
    let end = (start + SNIPPET_CHARS * 2).min(chars.len());
    let mut s: String = chars[start.min(chars.len())..end]
        .iter()
        .map(|&c| if c == '\n' { ' ' } else { c })
        .collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

pub async fn search(
    State(st): State<AppState>,
    MaybeUser(cu): MaybeUser,
    Query(q): Query<SearchQuery>,
) -> AppResult<Json<Vec<Hit>>> {
    let terms: Vec<String> = q
        .q
        .split_whitespace()
        .take(MAX_TERMS)
        .map(|t| t.chars().take(100).collect())
        .collect();
    if terms.is_empty() {
        return Ok(Json(vec![]));
    }
    // すべての語を含むページ（語ごとにタイトルか本文のどちらかに含まれればよい）
    let mut cond = Condition::all().add(page::Column::DeletedAt.is_null());
    // 見えないページ（下書き・公開範囲の外と、その子孫）を出さない
    let v = crate::access::viewer(&st.db, cu.as_ref()).await?;
    let hidden = pages::hidden_for(&st.db, &v).await?;
    if !hidden.is_empty() {
        cond = cond.add(page::Column::Id.is_not_in(hidden));
    }
    for t in &terms {
        let pat = like_pattern(t);
        cond = cond.add(
            Condition::any()
                .add(page::Column::Title.like(pat.as_str()))
                .add(page::Column::SearchText.like(pat.as_str())),
        );
    }
    let rows = page::Entity::find()
        .select_only()
        .columns([
            page::Column::Id,
            page::Column::Title,
            page::Column::Icon,
            page::Column::SearchText,
        ])
        .filter(cond)
        .order_by_desc(page::Column::UpdatedAt)
        .limit(MAX_RESULTS)
        .into_model::<Row>()
        .all(&st.db)
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| Hit {
                snippet: snippet(r.search_text.as_deref().unwrap_or(""), &terms),
                id: r.id,
                title: r.title,
                icon: r.icon,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_pattern("100%_a\\b"), "%100\\%\\_a\\\\b%");
    }

    #[test]
    fn snippet_centers_on_hit() {
        let text = format!("{}目印{}", "あ".repeat(100), "い".repeat(100));
        let s = snippet(&text, &["目印".into()]);
        assert!(s.starts_with('…') && s.ends_with('…'));
        assert!(s.contains("目印"));
        assert_eq!(snippet("短い本文", &["本文".into()]), "短い本文");
    }
}
