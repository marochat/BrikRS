//! GitHub Gist の埋め込み：Gist の URL から、GitHub の API でファイルの内容を取ってきて返す（取得は ogp.rs）。
//! 内容は挿入するときにブロックへ保存する（表示のたびに GitHub へ取りに行かない。API は認証なしだと
//! IP あたり 60回/時 まで。GitHub の埋め込み用 script も読み込まないので CSP を緩めずに済む）

use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    auth::AuthUser,
    error::{AppError, AppResult},
    ogp::{self, FetchError},
};

/// API の応答全体の上限（ファイルの内容を含む）
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// ブロックに保存する内容の上限（ページの保存の上限 4MB に余裕を持たせる）
const MAX_TOTAL_CHARS: usize = 256 * 1024;
const MAX_FILES: usize = 20;

#[derive(Deserialize)]
pub struct GistReq {
    url: String,
}

#[derive(Serialize)]
pub struct GistFile {
    name: String,
    language: String,
    content: String,
    /// 上限で切り詰めた（GitHub 側で切り詰められた場合も含む）
    truncated: bool,
}

#[derive(Serialize)]
pub struct Gist {
    id: String,
    /// Gist のページ（https://gist.github.com/...）
    url: String,
    owner: String,
    description: String,
    files: Vec<GistFile>,
}

/// Gist の ID：16進の 8〜40文字か、古い Gist の数字だけの ID
fn is_id(t: &str) -> bool {
    let hex = (8..=40).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_hexdigit());
    let numeric = (1..=10).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit());
    hex || numeric
}

/// Gist の URL（https://gist.github.com/<user>/<id>、末尾の .js や #file-…、版の指定 /<id>/<版> も可）か
/// ID そのものから ID を取り出す
fn gist_id(raw: &str) -> Option<String> {
    let s = raw.trim();
    if is_id(s) {
        return Some(s.to_ascii_lowercase());
    }
    let u = Url::parse(s).ok()?;
    if !matches!(u.scheme(), "http" | "https") || u.host_str()? != "gist.github.com" {
        return None;
    }
    let segs: Vec<&str> = u
        .path_segments()?
        .filter(|p| !p.is_empty())
        .map(|p| p.strip_suffix(".js").unwrap_or(p))
        .collect();
    // /<id> だけならそれ、/<user>/<id>/… なら 2番目（3番目以降の版のハッシュは ID ではない）
    let id = match segs[..] {
        [id] => id,
        [_, id, ..] => id,
        [] => return None,
    };
    is_id(id).then(|| id.to_ascii_lowercase())
}

pub async fn create(AuthUser(_): AuthUser, Json(req): Json<GistReq>) -> AppResult<Json<Gist>> {
    let id = gist_id(&req.url)
        .ok_or_else(|| AppError::BadRequest("Gist の URL（https://gist.github.com/…）として読み取れません".into()))?;
    let api = Url::parse(&format!("https://api.github.com/gists/{id}")).expect("固定の URL");
    let bytes = ogp::fetch_raw(api, "application/vnd.github+json", MAX_RESPONSE_BYTES)
        .await
        .map_err(|e| match e {
            FetchError::Rejected(m) => AppError::BadRequest(m.into()),
            FetchError::Network(m) => AppError::BadRequest(format!("Gist を読み込めませんでした：{m}")),
        })?;
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::BadRequest("Gist を読み込めませんでした（応答の形式が違います）".into()))?;
    Ok(Json(parse(&id, &v)))
}

/// API の応答から必要な分だけ取り出す。ファイルは GitHub の表示と同じくファイル名の順
fn parse(id: &str, v: &Value) -> Gist {
    let str_of = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let mut budget = MAX_TOTAL_CHARS;
    let mut files = Vec::new();
    if let Some(map) = v["files"].as_object() {
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (name, f) in entries.into_iter().take(MAX_FILES) {
            let full = f["content"].as_str().unwrap_or_default();
            let content: String = full.chars().take(budget).collect();
            let cut = content.len() < full.len();
            budget -= content.chars().count();
            files.push(GistFile {
                name: name.clone(),
                language: str_of(&f["language"]),
                content,
                truncated: cut || f["truncated"].as_bool().unwrap_or(false),
            });
        }
    }
    let url = v["html_url"]
        .as_str()
        .filter(|u| u.starts_with("https://gist.github.com/"))
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://gist.github.com/{id}"));
    Gist {
        id: id.to_string(),
        url,
        owner: str_of(&v["owner"]["login"]),
        description: str_of(&v["description"]),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_gist_urls() {
        let id = Some("aa5a315d61ae9438b18d".to_string());
        assert_eq!(gist_id("https://gist.github.com/octocat/aa5a315d61ae9438b18d"), id);
        assert_eq!(gist_id("https://gist.github.com/aa5a315d61ae9438b18d"), id);
        assert_eq!(gist_id("https://gist.github.com/octocat/aa5a315d61ae9438b18d.js"), id);
        assert_eq!(gist_id("https://gist.github.com/octocat/aa5a315d61ae9438b18d#file-hello-rs"), id);
        assert_eq!(gist_id("aa5a315d61ae9438b18d"), id);
        // 版を指定した URL は、版のハッシュではなく Gist の ID を取る
        assert_eq!(
            gist_id("https://gist.github.com/octocat/aa5a315d61ae9438b18d/57a7f021a713b1c5a6a199b54cc514735d2d462f"),
            id
        );
        // 古い Gist の数字だけの ID
        assert_eq!(gist_id("https://gist.github.com/octocat/12345"), Some("12345".to_string()));
        assert_eq!(gist_id("https://github.com/octocat/aa5a315d61ae9438b18d"), None);
        assert_eq!(gist_id("https://gist.github.com/octocat"), None);
        assert_eq!(gist_id("https://gist.github.com/"), None);
    }

    #[test]
    fn picks_files_in_name_order_and_caps_size() {
        let big = "x".repeat(MAX_TOTAL_CHARS + 10);
        let v = json!({
            "html_url": "https://gist.github.com/octocat/abc",
            "owner": { "login": "octocat" },
            "description": null,
            "files": {
                "b.txt": { "language": "Text", "content": big },
                "a.rs": { "language": "Rust", "content": "fn main() {}" }
            }
        });
        let g = parse("abc", &v);
        assert_eq!(g.owner, "octocat");
        assert_eq!(g.description, "");
        assert_eq!(g.files[0].name, "a.rs");
        assert!(!g.files[0].truncated);
        assert!(g.files[1].truncated);
        assert_eq!(g.files[0].content.len() + g.files[1].content.len(), MAX_TOTAL_CHARS);
    }
}
