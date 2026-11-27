//! 本文からの参照（ページリンク、アップロード画像）を取り出す。

use std::collections::BTreeSet;

use serde_json::Value;

/// 本文が参照しているページの id。`@` で挿入した pageLink と、`/p/<id>` へのリンクの両方を拾う
pub fn page_links(blocks: &Value) -> BTreeSet<i32> {
    let mut out = BTreeSet::new();
    walk_links(blocks, &mut out);
    out
}

fn walk_links(v: &Value, out: &mut BTreeSet<i32>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| walk_links(i, out)),
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("pageLink") {
                if let Some(id) = map
                    .get("props")
                    .and_then(|p| p.get("pageId"))
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse().ok())
                {
                    out.insert(id);
                }
            }
            if let Some(id) = map
                .get("href")
                .and_then(Value::as_str)
                .and_then(page_id_from_path)
            {
                out.insert(id);
            }
            map.values().for_each(|v| walk_links(v, out));
        }
        _ => {}
    }
}

fn page_id_from_path(href: &str) -> Option<i32> {
    href.strip_prefix("/p/")?.trim_end_matches('/').parse().ok()
}

/// 共有カード（OGP）に使う画像。本文で最初の画像ブロック、無ければ最初の文中の画像（表のセルの中など）
pub fn first_image(blocks: &Value) -> Option<String> {
    first_of(blocks, "image", "children").or_else(|| first_of(blocks, "inlineImage", ""))
}

/// type が kind で props.url のある最初の要素の URL。via が空なら全部のキーを辿る（表の content など）
fn first_of(v: &Value, kind: &str, via: &str) -> Option<String> {
    match v {
        Value::Array(items) => items.iter().find_map(|i| first_of(i, kind, via)),
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some(kind) {
                if let Some(url) = map.get("props").and_then(|p| p.get("url")).and_then(Value::as_str).filter(|u| !u.is_empty()) {
                    return Some(url.to_string());
                }
            }
            if via.is_empty() {
                map.values().find_map(|c| first_of(c, kind, via))
            } else {
                map.get(via).and_then(|c| first_of(c, kind, via))
            }
        }
        _ => None,
    }
}

/// 文字列（本文 JSON そのまま）に含まれる `/uploads/<保存名>` の保存名を集める。
/// 保存名は uploads.rs が作る「32桁の16進数 + 拡張子」
pub fn upload_names(text: &str, out: &mut BTreeSet<String>) {
    const PREFIX: &str = "/uploads/";
    let mut rest = text;
    while let Some(pos) = rest.find(PREFIX) {
        rest = &rest[pos + PREFIX.len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
            .collect();
        if name.len() > 33 && name.as_bytes()[32] == b'.' {
            out.insert(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collects_page_links() {
        let v = json!([
            {"id": "a", "type": "paragraph", "content": [
                {"type": "pageLink", "props": {"pageId": "3"}},
                {"type": "link", "href": "/p/7", "content": []},
                {"type": "link", "href": "https://example.com/p/9", "content": []}
            ], "children": [
                {"id": "b", "type": "paragraph", "content": [{"type": "pageLink", "props": {"pageId": "3"}}]}
            ]}
        ]);
        assert_eq!(page_links(&v).into_iter().collect::<Vec<_>>(), vec![3, 7]);
    }

    #[test]
    fn finds_first_image() {
        let v = json!([
            {"id": "a", "type": "paragraph", "content": [], "children": [
                {"id": "b", "type": "image", "props": {"url": "/uploads/x.png"}, "children": []}
            ]},
            {"id": "c", "type": "image", "props": {"url": "/uploads/y.png"}, "children": []}
        ]);
        assert_eq!(first_image(&v).as_deref(), Some("/uploads/x.png"));
        assert_eq!(first_image(&json!([])), None);
    }

    #[test]
    fn collects_upload_names() {
        let name = format!("{}.png", "a1".repeat(16));
        let text = format!(r#"{{"url":"/uploads/{name}"}} /uploads/short.png /uploads/"#);
        let mut out = BTreeSet::new();
        upload_names(&text, &mut out);
        assert_eq!(out.into_iter().collect::<Vec<_>>(), vec![name]);
    }

    #[test]
    fn first_image_falls_back_to_inline_image() {
        let table = json!([{"type": "table", "content": {"type": "tableContent", "rows": [{"cells": [
            {"type": "tableCell", "content": [{"type": "inlineImage", "props": {"url": "/uploads/t.png"}}]}
        ]}]}}]);
        assert_eq!(first_image(&table).as_deref(), Some("/uploads/t.png"));
        // 画像ブロックがあればそちらが先
        let mut both = table.as_array().unwrap().clone();
        both.push(json!({"type": "image", "props": {"url": "/uploads/b.png"}}));
        assert_eq!(first_image(&Value::Array(both)).as_deref(), Some("/uploads/b.png"));
    }
}
