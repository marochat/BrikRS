//! BlockNote のブロック JSON から検索用のプレーンテキストを取り出す。

use serde_json::Value;

pub fn extract(blocks: &Value) -> String {
    let mut out = String::new();
    walk(blocks, &mut out);
    // 連続する改行・空白を詰める
    out.split('\n')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn walk(v: &Value, out: &mut String) {
    match v {
        Value::Array(items) => {
            for item in items {
                walk(item, out);
                // ブロック（id を持つ要素）ごとに改行で区切る
                if item.get("id").is_some() {
                    out.push('\n');
                }
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get("text") {
                out.push_str(s);
            }
            // Web ブックマークは文字が props にしかないので、タイトルと説明を拾う
            if map.get("type").and_then(Value::as_str) == Some("bookmark") {
                let props = map.get("props");
                for key in ["title", "description"] {
                    if let Some(s) = props.and_then(|p| p.get(key)).and_then(Value::as_str) {
                        out.push_str(s);
                        out.push('\n');
                    }
                }
            }
            // Gist も説明とファイル（名前・内容）が props にしかない。ファイルは JSON 文字列で持っている
            if map.get("type").and_then(Value::as_str) == Some("gist") {
                let props = map.get("props");
                if let Some(s) = props.and_then(|p| p.get("description")).and_then(Value::as_str) {
                    out.push_str(s);
                    out.push('\n');
                }
                for f in gist_files(props) {
                    for key in ["name", "content"] {
                        if let Some(s) = f.get(key).and_then(Value::as_str) {
                            out.push_str(s);
                            out.push('\n');
                        }
                    }
                }
            }
            // X の投稿も投稿者と本文が props にしかない
            if map.get("type").and_then(Value::as_str) == Some("tweet") {
                let props = map.get("props");
                for key in ["author", "tweetHandle", "body"] {
                    if let Some(s) = props.and_then(|p| p.get(key)).and_then(Value::as_str) {
                        out.push_str(s);
                        out.push('\n');
                    }
                }
            }
            // Google マップは場所の名前（座標だけのときは座標）が props にしかない
            if map.get("type").and_then(Value::as_str) == Some("googleMap") {
                if let Some(s) = map.get("props").and_then(|p| p.get("mapsQ")).and_then(Value::as_str) {
                    out.push_str(s);
                    out.push('\n');
                }
            }
            // 数式も LaTeX が props にしかないので拾う（ブロック・インラインとも）
            if matches!(map.get("type").and_then(Value::as_str), Some("formula") | Some("inlineFormula")) {
                if let Some(s) = map.get("props").and_then(|p| p.get("formula")).and_then(Value::as_str) {
                    out.push_str(s);
                    out.push('\n');
                }
            }
            // serde_json の Map はキー順なので、本文 → 子ブロックの順を明示する
            for (k, val) in map {
                match k.as_str() {
                    // 色や配置などの属性、リンク先URLは検索対象にしない
                    "text" | "props" | "href" | "styles" | "children" => {}
                    _ => walk(val, out),
                }
            }
            if let Some(children) = map.get("children") {
                out.push('\n');
                walk(children, out);
            }
        }
        _ => {}
    }
}

/// gist ブロックの props.files（JSON 文字列）を読む。読めなければ空
pub fn gist_files(props: Option<&Value>) -> Vec<Value> {
    props
        .and_then(|p| p.get("files"))
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_nested_text() {
        let v = json!([
            {"id": "a", "type": "heading", "props": {"level": 1}, "content": [{"type": "text", "text": "見出し", "styles": {}}], "children": []},
            {"id": "b", "type": "paragraph", "content": [
                {"type": "text", "text": "本文と", "styles": {}},
                {"type": "link", "href": "https://x", "content": [{"type": "text", "text": "リンク", "styles": {}}]}
            ], "children": [
                {"id": "c", "type": "paragraph", "content": [{"type": "text", "text": "子", "styles": {}}], "children": []}
            ]},
            {"id": "d", "type": "table", "content": {"type": "tableContent", "rows": [{"cells": [[{"type": "text", "text": "セル", "styles": {}}]]}]}}
        ]);
        assert_eq!(extract(&v), "見出し\n本文とリンク\n子\nセル");
    }

    #[test]
    fn extracts_bookmark_text() {
        let v = json!([
            {"id": "a", "type": "bookmark", "props": {"url": "https://x", "title": "記事", "description": "説明", "image": ""}, "children": []}
        ]);
        assert_eq!(extract(&v), "記事\n説明");
    }

    #[test]
    fn extracts_formula_text() {
        let v = json!([
            {"id": "a", "type": "formula", "props": {"formula": "x^2+y^2=z^2"}, "children": []},
            {"id": "b", "type": "paragraph", "content": [
                {"type": "text", "text": "本文", "styles": {}},
                {"type": "inlineFormula", "props": {"formula": "E=mc^2"}}
            ], "children": []}
        ]);
        assert_eq!(extract(&v), "x^2+y^2=z^2\n本文E=mc^2");
    }
}
