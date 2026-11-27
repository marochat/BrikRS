//! BlockNote のブロック JSON を保存前に検査・無害化する。
//!
//! 表示側は BlockNote(ProseMirror) が DOM を組み立てるため生 HTML は解釈されないが、
//! リンクや画像の URL に `javascript:` などを仕込まれる余地は残る。
//! そこで URL を持つキーを走査し、許可したスキームだけを残す。
//! YouTube の動画 ID（videoId）、Google マップの props（mapsPb など）、X の投稿の ID と @名前
//! （tweetId・tweetHandle）は表示側で URL に組み立てるので、決まった形でなければ空にする。

use serde_json::Value;

use crate::{gmaps, tweet, youtube::is_video_id};

/// URL として扱うキー（リンクの href、画像・ファイルブロックの url、Web ブックマークの image など）
const URL_KEYS: &[&str] = &["href", "url", "src", "image"];
const MAX_DEPTH: usize = 64;

pub fn sanitize_blocks(mut v: Value) -> Result<Value, &'static str> {
    if !v.is_array() {
        return Err("content はブロックの配列である必要があります");
    }
    walk(&mut v, 0)?;
    Ok(v)
}

fn walk(v: &mut Value, depth: usize) -> Result<(), &'static str> {
    if depth > MAX_DEPTH {
        return Err("content の入れ子が深すぎます");
    }
    match v {
        Value::Array(items) => {
            for item in items {
                walk(item, depth + 1)?;
            }
        }
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                let check: Option<fn(&str) -> bool> = match k.as_str() {
                    "videoId" => Some(is_video_id),
                    "tweetId" => Some(tweet::is_tweet_id),
                    "tweetHandle" => Some(tweet::is_handle),
                    _ => None,
                };
                if let Some(ok) = check {
                    match val {
                        // 空は「まだ設定されていない」・「@名前が分からない」で、そのまま
                        Value::String(s) if s.is_empty() || ok(s) => {}
                        _ => *val = Value::String(String::new()),
                    }
                } else if let Some(ok) = gmaps::check_prop(k, val.as_str().unwrap_or("")) {
                    if !ok || !val.is_string() {
                        *val = Value::String(String::new());
                    }
                } else if URL_KEYS.contains(&k.as_str()) {
                    match val {
                        Value::String(s) => {
                            if !is_safe_url(s) {
                                *s = String::new();
                            }
                        }
                        // オブジェクト/配列ならキー名で素通りさせず、中の href/url/src もふつうに検査する
                        _ => walk(val, depth + 1)?,
                    }
                } else {
                    walk(val, depth + 1)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn is_safe_url(raw: &str) -> bool {
    // 制御文字や空白を挟んで "java\tscript:" のようにすり抜けるのを防ぐ
    let s: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    if s.is_empty() {
        return true;
    }
    if s.starts_with('/') || s.starts_with('#') || s.starts_with('?') {
        // "//evil.example" のようなプロトコル相対URLは http(s) と同等なので許可してよい
        return true;
    }
    match s.split_once(':') {
        Some((scheme, _)) => matches!(scheme, "http" | "https" | "mailto"),
        // スキームなしの相対パス
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_dangerous_schemes() {
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url(" JaVa\tScRiPt:alert(1)"));
        assert!(!is_safe_url("data:text/html,<script>"));
        assert!(!is_safe_url("vbscript:x"));
        assert!(is_safe_url("https://example.com/a?b=c"));
        assert!(is_safe_url("/p/3"));
        assert!(is_safe_url("mailto:a@example.com"));
    }

    #[test]
    fn strips_bad_href_in_nested_blocks() {
        let input = json!([{
            "type": "paragraph",
            "content": [{"type": "link", "href": "javascript:alert(1)", "content": []}],
            "children": [{"type": "image", "props": {"url": "https://ok.example/a.png"}}]
        }]);
        let out = sanitize_blocks(input).unwrap();
        assert_eq!(out[0]["content"][0]["href"], "");
        assert_eq!(out[0]["children"][0]["props"]["url"], "https://ok.example/a.png");
    }

    #[test]
    fn requires_array() {
        assert!(sanitize_blocks(json!({"a": 1})).is_err());
    }

    #[test]
    fn strips_bad_href_nested_under_url_key() {
        // URL_KEYS に一致するキーの値がオブジェクトでも、素通りさせずに中の href まで検査する
        let input = json!([{
            "type": "bookmark",
            "props": { "image": { "href": "javascript:alert(1)" } }
        }]);
        let out = sanitize_blocks(input).unwrap();
        assert_eq!(out[0]["props"]["image"]["href"], "");
    }

    #[test]
    fn clears_invalid_video_id() {
        let input = json!([
            { "type": "youtube", "props": { "videoId": "dQw4w9WgXcQ" } },
            { "type": "youtube", "props": { "videoId": "x\"><script>" } },
            { "type": "youtube", "props": { "videoId": { "a": 1 } } }
        ]);
        let out = sanitize_blocks(input).unwrap();
        assert_eq!(out[0]["props"]["videoId"], "dQw4w9WgXcQ");
        assert_eq!(out[1]["props"]["videoId"], "");
        assert_eq!(out[2]["props"]["videoId"], "");
    }

    #[test]
    fn clears_invalid_tweet_props() {
        let input = json!([
            { "type": "tweet", "props": { "tweetId": "1585341984679469056", "tweetHandle": "elonmusk" } },
            { "type": "tweet", "props": { "tweetId": "20", "tweetHandle": "" } },
            { "type": "tweet", "props": { "tweetId": "1\"><script>", "tweetHandle": "a/../b" } },
            { "type": "tweet", "props": { "tweetId": 20, "tweetHandle": { "a": 1 } } }
        ]);
        let out = sanitize_blocks(input).unwrap();
        assert_eq!(out[0]["props"]["tweetId"], "1585341984679469056");
        assert_eq!(out[0]["props"]["tweetHandle"], "elonmusk");
        assert_eq!(out[1]["props"]["tweetId"], "20");
        for i in [2, 3] {
            assert_eq!(out[i]["props"]["tweetId"], "");
            assert_eq!(out[i]["props"]["tweetHandle"], "");
        }
    }

    #[test]
    fn clears_invalid_map_props() {
        let input = json!([
            { "type": "googleMap", "props": { "mapsPb": "!1m3!1d3240.8", "mapsMid": "", "mapsQ": "東京駅", "mapsLl": "35.68,139.76", "mapsZoom": "17" } },
            { "type": "googleMap", "props": { "mapsPb": "!1\"><script>", "mapsMid": "a b", "mapsQ": "x\u{0}", "mapsLl": "999,0", "mapsZoom": { "a": 1 } } }
        ]);
        let out = sanitize_blocks(input).unwrap();
        assert_eq!(out[0]["props"]["mapsPb"], "!1m3!1d3240.8");
        assert_eq!(out[0]["props"]["mapsQ"], "東京駅");
        assert_eq!(out[0]["props"]["mapsZoom"], "17");
        for k in ["mapsPb", "mapsMid", "mapsQ", "mapsLl", "mapsZoom"] {
            assert_eq!(out[1]["props"][k], "", "{k}");
        }
    }
}
