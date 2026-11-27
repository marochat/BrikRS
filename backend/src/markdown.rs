//! Markdown と BlockNote のブロック JSON の相互変換（CLI からの投稿・読み出し用）。
//!
//! 外部ツール（Claude Code など）が書いた記事を取り込み、読み出せれば十分なので、
//! 見出し・段落・リスト・チェックリスト・引用・コード・区切り線・画像・表・数式（`$$…$$`）と、
//! 太字・斜体・取り消し線・インラインコード・リンク・インライン数式（`$…$`）だけを扱う。
//! YouTube の動画の URL（またはそこへのリンク）だけの段落は、動画の埋め込み（youtube ブロック）にする。
//! Google マップの URL だけの段落も、地図の埋め込み（googleMap ブロック）にする。
//! X の投稿の URL だけの段落は、投稿の ID だけを持つ tweet ブロックにする（取り込みでは X へ取りに行かない。
//! 本文は編集画面の「↻」で取れる）。
//! 段落内の改行は Markdown の仕様（空白扱い）ではなく、そのまま改行として残す。

use std::collections::HashMap;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{Value, json};

use crate::{auth, gmaps, plaintext, tweet, youtube};

// ---------- Markdown → ブロック ----------

#[derive(Default, Clone, Copy)]
struct Styles {
    bold: bool,
    italic: bool,
    strike: bool,
}

enum Kind {
    Root,
    Paragraph,
    Heading(u8),
    Quote,
    Code(String),
    List { ordered: bool },
    Item { ordered: bool },
    Table,
    Cell,
    Image(String),
}

struct Frame {
    kind: Kind,
    inline: Vec<Value>,
    children: Vec<Value>,
    /// 段落の中に出てきた画像・数式ブロックなど、このブロックの後ろに出すブロック
    after: Vec<Value>,
    checked: Option<bool>,
    /// 表の行（各行はセルの配列）
    rows: Vec<Value>,
    cells: Vec<Value>,
}

impl Frame {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            inline: Vec::new(),
            children: Vec::new(),
            after: Vec::new(),
            checked: None,
            rows: Vec::new(),
            cells: Vec::new(),
        }
    }
}

fn bid() -> String {
    auth::random_token()[..12].to_string()
}

fn block(kind: &str, props: Value, content: Value, children: Vec<Value>) -> Value {
    json!({ "id": bid(), "type": kind, "props": props, "content": content, "children": children })
}

fn text(s: &str, st: Styles, code: bool) -> Value {
    let mut styles = serde_json::Map::new();
    for (on, key) in [(st.bold, "bold"), (st.italic, "italic"), (st.strike, "strike"), (code, "code")] {
        if on {
            styles.insert(key.into(), Value::Bool(true));
        }
    }
    json!({ "type": "text", "text": s, "styles": styles })
}

/// 段落の中身が URL だけ（素の URL か、リンク1つ）なら、その URL
fn only_url(inline: &[Value]) -> Option<&str> {
    let [item] = inline else { return None };
    match item["type"].as_str()? {
        "text" => item["text"].as_str(),
        "link" => item["href"].as_str(),
        _ => None,
    }
}

/// URL だけの段落を埋め込み（YouTube・X の投稿・Google マップ）にする
fn embed_block(inline: &[Value]) -> Option<Value> {
    let url = only_url(inline)?;
    if let Some(id) = youtube::video_id(url) {
        return Some(block("youtube", json!({ "videoId": id }), json!([]), vec![]));
    }
    // ID そのもの（数字だけの段落）は投稿にしない
    if let Some(id) = tweet::tweet_id(url).filter(|_| url.contains('/')) {
        return Some(block("tweet", json!({ "tweetId": id }), json!([]), vec![]));
    }
    let m = gmaps::parse(url)?;
    let props = json!({ "mapsPb": m.pb, "mapsMid": m.mid, "mapsQ": m.q, "mapsLl": m.ll, "mapsZoom": m.zoom });
    Some(block("googleMap", props, json!([]), vec![]))
}

/// Markdown を BlockNote のブロック配列にする
pub fn to_blocks(src: &str) -> Value {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH;
    let mut stack = vec![Frame::new(Kind::Root)];
    let mut st = Styles::default();
    // 開いているリンク（href と中身）
    let mut links: Vec<(String, Vec<Value>)> = Vec::new();
    // リスト項目の中の最初の段落は、項目そのものの本文として扱う
    let mut merged_paragraphs: Vec<bool> = Vec::new();

    let push_inline = |stack: &mut Vec<Frame>, links: &mut Vec<(String, Vec<Value>)>, v: Value| {
        match links.last_mut() {
            Some((_, content)) => content.push(v),
            None => stack.last_mut().unwrap().inline.push(v),
        }
    };

    for ev in Parser::new_ext(src, opts) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    let top = stack.last().unwrap();
                    let merge = matches!(top.kind, Kind::Item { .. })
                        && top.inline.is_empty()
                        && top.children.is_empty();
                    merged_paragraphs.push(merge);
                    if !merge {
                        stack.push(Frame::new(Kind::Paragraph));
                    }
                }
                Tag::Heading { level, .. } => {
                    let n = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    stack.push(Frame::new(Kind::Heading(n)));
                }
                Tag::BlockQuote(_) => stack.push(Frame::new(Kind::Quote)),
                Tag::CodeBlock(k) => {
                    let lang = match k {
                        CodeBlockKind::Fenced(l) => l.split_whitespace().next().unwrap_or("").to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    stack.push(Frame::new(Kind::Code(lang)));
                }
                Tag::List(start) => stack.push(Frame::new(Kind::List { ordered: start.is_some() })),
                Tag::Item => {
                    let ordered = match stack.last().map(|f| &f.kind) {
                        Some(Kind::List { ordered }) => *ordered,
                        _ => false,
                    };
                    stack.push(Frame::new(Kind::Item { ordered }));
                }
                Tag::Table(_) => stack.push(Frame::new(Kind::Table)),
                Tag::TableCell => stack.push(Frame::new(Kind::Cell)),
                Tag::Emphasis => st.italic = true,
                Tag::Strong => st.bold = true,
                Tag::Strikethrough => st.strike = true,
                Tag::Link { dest_url, .. } => links.push((dest_url.to_string(), Vec::new())),
                Tag::Image { dest_url, .. } => stack.push(Frame::new(Kind::Image(dest_url.to_string()))),
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => {
                    if merged_paragraphs.pop() == Some(false) {
                        let f = stack.pop().unwrap();
                        let parent = stack.last_mut().unwrap();
                        // 画像や数式ブロックだけの段落は、空の段落を残さない
                        let only_blocks = f.inline.is_empty() && !f.after.is_empty();
                        if let Some(b) = embed_block(&f.inline).filter(|_| f.after.is_empty()) {
                            parent.children.push(b);
                        } else if !only_blocks {
                            parent.children.push(block("paragraph", json!({}), json!(f.inline), vec![]));
                        }
                        parent.children.extend(f.after);
                    }
                }
                TagEnd::Heading(_) => {
                    let f = stack.pop().unwrap();
                    let Kind::Heading(n) = f.kind else { unreachable!() };
                    let parent = stack.last_mut().unwrap();
                    parent.children.push(block("heading", json!({ "level": n }), json!(f.inline), vec![]));
                    parent.children.extend(f.after);
                }
                TagEnd::BlockQuote(_) => {
                    // 引用は1ブロックに本文を持つ形なので、中の段落を改行でつなぐ
                    let f = stack.pop().unwrap();
                    let mut content = Vec::new();
                    let mut rest = Vec::new();
                    for c in f.children {
                        if c["type"] == "paragraph" {
                            if !content.is_empty() {
                                content.push(text("\n", Styles::default(), false));
                            }
                            content.extend(c["content"].as_array().cloned().unwrap_or_default());
                        } else {
                            rest.push(c);
                        }
                    }
                    let parent = stack.last_mut().unwrap();
                    parent.children.push(block("quote", json!({}), json!(content), vec![]));
                    parent.children.extend(rest);
                }
                TagEnd::CodeBlock => {
                    let f = stack.pop().unwrap();
                    let Kind::Code(lang) = f.kind else { unreachable!() };
                    let src: String = f.inline.iter().filter_map(|v| v["text"].as_str()).collect();
                    let src = src.strip_suffix('\n').unwrap_or(&src);
                    stack.last_mut().unwrap().children.push(block(
                        "codeBlock",
                        json!({ "language": lang }),
                        json!([text(src, Styles::default(), false)]),
                        vec![],
                    ));
                }
                TagEnd::List(_) => {
                    let f = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.extend(f.children);
                }
                TagEnd::Item => {
                    let f = stack.pop().unwrap();
                    let Kind::Item { ordered } = f.kind else { unreachable!() };
                    let (kind, props) = match f.checked {
                        Some(c) => ("checkListItem", json!({ "checked": c })),
                        None if ordered => ("numberedListItem", json!({})),
                        None => ("bulletListItem", json!({})),
                    };
                    let mut children = f.after;
                    children.extend(f.children);
                    stack.last_mut().unwrap().children.push(block(kind, props, json!(f.inline), children));
                }
                TagEnd::Table => {
                    let f = stack.pop().unwrap();
                    let cols = f.rows.first().and_then(|r| r["cells"].as_array()).map_or(0, Vec::len);
                    stack.last_mut().unwrap().children.push(json!({
                        "id": bid(), "type": "table", "props": {}, "children": [],
                        "content": {
                            "type": "tableContent",
                            "columnWidths": vec![Value::Null; cols],
                            "headerRows": 1,
                            "rows": f.rows,
                        },
                    }));
                }
                TagEnd::TableHead | TagEnd::TableRow => {
                    let f = stack.last_mut().unwrap();
                    let cells = std::mem::take(&mut f.cells);
                    f.rows.push(json!({ "cells": cells }));
                }
                TagEnd::TableCell => {
                    let f = stack.pop().unwrap();
                    stack.last_mut().unwrap().cells.push(json!({
                        "type": "tableCell", "props": {}, "content": f.inline,
                    }));
                }
                TagEnd::Emphasis => st.italic = false,
                TagEnd::Strong => st.bold = false,
                TagEnd::Strikethrough => st.strike = false,
                TagEnd::Link => {
                    if let Some((href, content)) = links.pop() {
                        push_inline(&mut stack, &mut links, json!({ "type": "link", "href": href, "content": content }));
                    }
                }
                TagEnd::Image => {
                    // 画像は独立したブロックなので、段落の後ろに出す
                    let f = stack.pop().unwrap();
                    let Kind::Image(url) = f.kind else { unreachable!() };
                    let alt: String = f.inline.iter().filter_map(|v| v["text"].as_str()).collect();
                    let name = url.rsplit('/').next().unwrap_or("").to_string();
                    stack.last_mut().unwrap().after.push(json!({
                        "id": bid(), "type": "image", "children": [],
                        "props": { "url": url, "caption": alt, "name": name },
                    }));
                }
                _ => {}
            },
            Event::Text(s) | Event::Html(s) | Event::InlineHtml(s) => {
                if matches!(stack.last().unwrap().kind, Kind::Code(_) | Kind::Image(_)) {
                    stack.last_mut().unwrap().inline.push(json!({ "text": s.as_ref() }));
                } else {
                    push_inline(&mut stack, &mut links, text(&s, st, false));
                }
            }
            Event::Code(s) => push_inline(&mut stack, &mut links, text(&s, st, true)),
            Event::InlineMath(s) => push_inline(
                &mut stack,
                &mut links,
                json!({ "type": "inlineFormula", "props": { "formula": s.trim() } }),
            ),
            // 数式ブロックは独立したブロックなので、画像と同じく段落の後ろに出す
            Event::DisplayMath(s) => stack.last_mut().unwrap().after.push(json!({
                "id": bid(), "type": "formula", "props": { "formula": s.trim() }, "children": [],
            })),
            Event::SoftBreak | Event::HardBreak => {
                push_inline(&mut stack, &mut links, text("\n", Styles::default(), false))
            }
            Event::Rule => stack.last_mut().unwrap().children.push(json!({
                "id": bid(), "type": "divider", "props": {}, "children": [],
            })),
            Event::TaskListMarker(c) => stack.last_mut().unwrap().checked = Some(c),
            _ => {}
        }
    }
    let root = stack.into_iter().next().unwrap();
    Value::Array(root.children)
}

// ---------- ブロック → Markdown ----------

/// ブロック配列を Markdown にする。`titles` はページリンクの表示名に使う
pub fn from_blocks(blocks: &Value, titles: &HashMap<i32, String>) -> String {
    let mut out = String::new();
    write_blocks(blocks, "", titles, &mut out);
    out.trim_end().to_string() + "\n"
}

/// リスト項目の記号の種類（`-` か `1.` か）。リスト以外は None
fn list_mark(t: &str) -> Option<&'static str> {
    match t {
        "bulletListItem" | "checkListItem" | "toggleListItem" => Some("-"),
        "numberedListItem" => Some("1."),
        _ => None,
    }
}

fn write_blocks(blocks: &Value, indent: &str, titles: &HashMap<i32, String>, out: &mut String) {
    let Some(items) = blocks.as_array() else { return };
    let mut prev_mark = None;
    let mut number = 0;
    for b in items {
        let t = b["type"].as_str().unwrap_or("");
        let props = &b["props"];
        let content = &b["content"];
        let mark = list_mark(t);
        let list = mark.is_some();
        if t == "numberedListItem" {
            number += 1;
        } else {
            number = 0;
        }
        // 段落など、空のブロックは書かない
        let body = inline_md(content, titles);
        if t == "paragraph" && body.trim().is_empty() && b["children"].as_array().is_none_or(Vec::is_empty) {
            continue;
        }
        // 同じ種類のリスト項目どうしは詰め、それ以外は空行で区切る
        if !out.is_empty() && !(list && mark == prev_mark) {
            out.push('\n');
        }
        prev_mark = mark;
        let line = |prefix: &str, s: &str| -> String {
            let cont = format!("\n{indent}{}", " ".repeat(prefix.chars().count()));
            format!("{indent}{prefix}{}\n", s.replace('\n', &cont))
        };
        let mut child_indent = format!("{indent}  ");
        match t {
            "heading" => {
                let n = props["level"].as_u64().unwrap_or(1).clamp(1, 6) as usize;
                out.push_str(&line(&format!("{} ", "#".repeat(n)), &body));
            }
            "bulletListItem" | "toggleListItem" => out.push_str(&line("- ", &body)),
            "numberedListItem" => {
                let p = format!("{number}. ");
                child_indent = format!("{indent}{}", " ".repeat(p.len()));
                out.push_str(&line(&p, &body));
            }
            "checkListItem" => {
                let mark = if props["checked"].as_bool() == Some(true) { "x" } else { " " };
                out.push_str(&line(&format!("- [{mark}] "), &body));
            }
            "quote" => out.push_str(&line("> ", &body.replace('\n', "\n> "))),
            "codeBlock" => {
                let lang = props["language"].as_str().unwrap_or("");
                let src: String = content
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v["text"].as_str()).collect())
                    .unwrap_or_default();
                let fence = if src.contains("```") { "````" } else { "```" };
                out.push_str(&format!("{indent}{fence}{lang}\n"));
                for l in src.lines() {
                    out.push_str(&format!("{indent}{l}\n"));
                }
                out.push_str(&format!("{indent}{fence}\n"));
            }
            "divider" => out.push_str(&format!("{indent}---\n")),
            "image" | "video" | "audio" | "file" => {
                let url = props["url"].as_str().unwrap_or("");
                let cap = props["caption"].as_str().filter(|s| !s.is_empty());
                let name = cap.or(props["name"].as_str()).unwrap_or("");
                let bang = if t == "image" { "!" } else { "" };
                out.push_str(&format!("{indent}{bang}[{name}]({url})\n"));
            }
            "bookmark" => {
                let url = props["url"].as_str().unwrap_or("");
                let title = props["title"].as_str().filter(|s| !s.is_empty()).unwrap_or(url);
                out.push_str(&format!("{indent}[{title}]({url})\n"));
            }
            "linkCard" => {
                // linkCard はタイトルを props ではなく content（インライン）に持つ
                let url = props["url"].as_str().unwrap_or("");
                let title = if body.trim().is_empty() { url } else { body.trim() };
                out.push_str(&format!("{indent}[{title}]({url})\n"));
            }
            "gist" => {
                // リンクと、各ファイルをコードブロックで（読み出した側で中身も読めるように）
                let url = props["url"].as_str().unwrap_or("");
                let desc = props["description"].as_str().filter(|s| !s.is_empty()).unwrap_or("Gist");
                out.push_str(&format!("{indent}[{desc}]({url})\n"));
                for f in plaintext::gist_files(Some(props)) {
                    let name = f["name"].as_str().unwrap_or("");
                    let src = f["content"].as_str().unwrap_or("");
                    let fence = if src.contains("```") { "````" } else { "```" };
                    out.push_str(&format!("\n{indent}{fence}\n"));
                    for l in src.lines() {
                        out.push_str(&format!("{indent}{l}\n"));
                    }
                    out.push_str(&format!("{indent}{fence}\n{indent}（{name}）\n"));
                }
            }
            "youtube" => {
                if let Some(id) = props["videoId"].as_str().filter(|s| !s.is_empty()) {
                    out.push_str(&format!("{indent}[YouTube](https://www.youtube.com/watch?v={id})\n"));
                }
            }
            "tweet" => {
                // 本文を引用で、その後ろに投稿へのリンク（取り込むと tweet ブロックに戻る）
                if let Some(id) = props["tweetId"].as_str().filter(|s| !s.is_empty()) {
                    let handle = props["tweetHandle"].as_str().unwrap_or("");
                    let body = props["body"].as_str().unwrap_or("").trim();
                    if !body.is_empty() {
                        let author = props["author"].as_str().unwrap_or("");
                        let date = props["date"].as_str().unwrap_or("");
                        let who = match (author.is_empty(), handle.is_empty()) {
                            (false, false) => format!("{author} (@{handle})"),
                            (false, true) => author.to_string(),
                            (true, false) => format!("@{handle}"),
                            (true, true) => String::new(),
                        };
                        let sign = [who.as_str(), date].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
                        let quote = if sign.is_empty() { body.to_string() } else { format!("{body}\n\n— {sign}") };
                        for l in quote.lines() {
                            out.push_str(&format!("{indent}> {l}\n"));
                        }
                        out.push('\n');
                    }
                    out.push_str(&format!("{indent}[X の投稿]({})\n", tweet::status_url(id, handle)));
                }
            }
            "googleMap" => {
                let get = |k: &str| props[k].as_str().unwrap_or("").to_string();
                let m = gmaps::MapProps {
                    pb: get("mapsPb"),
                    mid: get("mapsMid"),
                    q: get("mapsQ"),
                    ll: get("mapsLl"),
                    zoom: get("mapsZoom"),
                };
                if let Some(url) = gmaps::open_url(&m) {
                    out.push_str(&format!("{indent}[Google マップ]({url})\n"));
                }
            }
            "formula" => {
                let f = props["formula"].as_str().unwrap_or("");
                out.push_str(&format!("{indent}$$\n{indent}{f}\n{indent}$$\n"));
            }
            "table" => {
                let rows = content["rows"].as_array().cloned().unwrap_or_default();
                for (i, r) in rows.iter().enumerate() {
                    let cells: Vec<String> = r["cells"]
                        .as_array()
                        .map(|cs| {
                            cs.iter()
                                .map(|c| {
                                    // セルは tableCell か、古い形式ではインライン配列そのもの
                                    let inner = if c.is_array() { c } else { &c["content"] };
                                    inline_md(inner, titles).replace('\n', "<br>").replace('|', "\\|")
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    out.push_str(&format!("{indent}| {} |\n", cells.join(" | ")));
                    if i == 0 {
                        out.push_str(&format!("{indent}|{}\n", " --- |".repeat(cells.len())));
                    }
                }
            }
            "paragraph" => out.push_str(&line("", &body)),
            // 知らないブロックは文字だけ残す
            _ => {
                let s = if body.is_empty() { plaintext::extract(&json!([b])) } else { body };
                if !s.is_empty() {
                    out.push_str(&line("", &s));
                }
            }
        }
        if let Some(children) = b["children"].as_array().filter(|c| !c.is_empty()) {
            let mut sub = String::new();
            write_blocks(&Value::Array(children.clone()), &child_indent, titles, &mut sub);
            if !list {
                out.push('\n');
            }
            out.push_str(&sub);
        }
    }
}

fn inline_md(content: &Value, titles: &HashMap<i32, String>) -> String {
    let Some(items) = content.as_array() else { return String::new() };
    let mut out = String::new();
    for v in items {
        match v["type"].as_str().unwrap_or("") {
            "text" => {
                let s = v["text"].as_str().unwrap_or("");
                let st = &v["styles"];
                if st["code"] == true {
                    out.push('`');
                    out.push_str(s);
                    out.push('`');
                    continue;
                }
                // 前後の空白は記号の外に出す（`** a**` は太字にならない）
                let core = s.trim();
                if core.is_empty() {
                    out.push_str(s);
                    continue;
                }
                let lead = &s[..s.len() - s.trim_start().len()];
                let trail = &s[s.trim_end().len()..];
                let mut marks = String::new();
                for (key, m) in [("bold", "**"), ("italic", "*"), ("strike", "~~")] {
                    if st[key] == true {
                        marks.push_str(m);
                    }
                }
                let close: String = marks.chars().rev().collect();
                out.push_str(&format!("{lead}{marks}{core}{close}{trail}"));
            }
            "link" => {
                let href = v["href"].as_str().unwrap_or("");
                out.push_str(&format!("[{}]({href})", inline_md(&v["content"], titles)));
            }
            "pageLink" => {
                let id: i32 = v["props"]["pageId"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
                let title = titles.get(&id).map_or("（削除されたページ）", String::as_str);
                out.push_str(&format!("[{title}](/p/{id})"));
            }
            "inlineFormula" => {
                out.push_str(&format!("${}$", v["props"]["formula"].as_str().unwrap_or("")));
            }
            // 文中・表のセルの画像（InlineImage.tsx）
            "inlineImage" => {
                let url = v["props"]["url"].as_str().unwrap_or("");
                if !url.is_empty() {
                    out.push_str(&format!("![{}]({url})", v["props"]["name"].as_str().unwrap_or("")));
                }
            }
            _ => {
                if let Some(s) = v["text"].as_str() {
                    out.push_str(s);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_markdown() {
        let v = to_blocks(
            "# 見出し\n\n本文 **太字** と [リンク](https://x)\n次の行\n\n- a\n  - b\n- [x] 済み\n\n1. 一\n2. 二\n\n> 引用\n\n```rust\nfn main() {}\n```\n\n---\n\n![図](/uploads/a.png)\n\n| A | B |\n|---|---|\n| 1 | 2 |\n",
        );
        let types: Vec<&str> = v.as_array().unwrap().iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            [
                "heading", "paragraph", "bulletListItem", "checkListItem", "numberedListItem",
                "numberedListItem", "quote", "codeBlock", "divider", "image", "table"
            ]
        );
        assert_eq!(v[0]["props"]["level"], 1);
        assert_eq!(v[1]["content"][1]["styles"]["bold"], true);
        assert_eq!(v[1]["content"][3]["href"], "https://x");
        assert_eq!(v[1]["content"][4]["text"], "\n");
        assert_eq!(v[2]["children"][0]["type"], "bulletListItem");
        assert_eq!(v[3]["props"]["checked"], true);
        assert_eq!(v[7]["props"]["language"], "rust");
        assert_eq!(v[7]["content"][0]["text"], "fn main() {}");
        assert_eq!(v[9]["props"]["url"], "/uploads/a.png");
        assert_eq!(v[9]["props"]["caption"], "図");
        assert_eq!(v[10]["content"]["rows"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn converts_math() {
        let v = to_blocks("面積は $\\pi r^2$ です\n\n$$\nE = mc^2\n$$\n\n$5 と $10\n");
        assert_eq!(v[0]["content"][1]["type"], "inlineFormula");
        assert_eq!(v[0]["content"][1]["props"]["formula"], "\\pi r^2");
        assert_eq!(v[1]["type"], "formula");
        assert_eq!(v[1]["props"]["formula"], "E = mc^2");
        // 金額の `$` は数式にならない
        let plain: String = v[2]["content"].as_array().unwrap().iter().filter_map(|t| t["text"].as_str()).collect();
        assert_eq!(plain, "$5 と $10");
        let back = from_blocks(&v, &HashMap::new());
        assert_eq!(to_blocks(&back)[1]["props"]["formula"], "E = mc^2");
    }

    #[test]
    fn loose_list_items_keep_text() {
        let v = to_blocks("- a\n\n- b\n");
        assert_eq!(v[0]["content"][0]["text"], "a");
        assert_eq!(v[1]["content"][0]["text"], "b");
    }

    #[test]
    fn round_trips_to_markdown() {
        let md = "# 見出し\n\n本文 **太字** と *斜体* と `code` と [リンク](https://x)\n\n- a\n  - b\n- [ ] c\n\n1. 一\n2. 二\n\n> 引用\n\n```rust\nfn main() {}\n```\n\n---\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n";
        let back = from_blocks(&to_blocks(md), &HashMap::new());
        assert_eq!(back, md);
    }

    #[test]
    fn page_links_use_titles() {
        let v = json!([{ "id": "a", "type": "paragraph", "props": {}, "children": [],
            "content": [{ "type": "pageLink", "props": { "pageId": "3" } }] }]);
        let titles = HashMap::from([(3, "使い方".to_string())]);
        assert_eq!(from_blocks(&v, &titles), "[使い方](/p/3)\n");
    }

    #[test]
    fn youtube_paragraph_becomes_embed() {
        let blocks = to_blocks(
            "https://youtu.be/dQw4w9WgXcQ\n\n[YouTube](https://www.youtube.com/watch?v=dQw4w9WgXcQ)\n\n\
             見て https://youtu.be/dQw4w9WgXcQ\n",
        );
        assert_eq!(blocks[0]["type"], "youtube");
        assert_eq!(blocks[0]["props"]["videoId"], "dQw4w9WgXcQ");
        // 書き出し（[YouTube](…)）からの取り込みも埋め込みに戻る
        assert_eq!(blocks[1]["type"], "youtube");
        // 文の中の URL はそのまま
        assert_eq!(blocks[2]["type"], "paragraph");
    }

    #[test]
    fn gist_as_link_and_code() {
        let files = json!([{ "name": "a.rs", "language": "Rust", "content": "fn main() {}" }]).to_string();
        let g = json!([{ "type": "gist", "props": { "url": "https://gist.github.com/o/abc", "description": "例", "files": files }, "content": [] }]);
        assert_eq!(from_blocks(&g, &HashMap::new()), "[例](https://gist.github.com/o/abc)\n\n```\nfn main() {}\n```\n（a.rs）\n");
    }

    #[test]
    fn tweet_url_paragraph_becomes_embed_with_id_only() {
        let blocks = to_blocks(
            "https://x.com/jack/status/20\n\n[X の投稿](https://twitter.com/jack/status/20?s=20)\n\n20\n\nhttps://x.com/jack\n",
        );
        assert_eq!(blocks[0]["type"], "tweet");
        assert_eq!(blocks[0]["props"], json!({ "tweetId": "20" }));
        assert_eq!(blocks[1]["type"], "tweet");
        // 数字だけの段落・投稿でない URL は段落のまま
        assert_eq!(blocks[2]["type"], "paragraph");
        assert_eq!(blocks[3]["type"], "paragraph");
    }

    #[test]
    fn tweet_as_quote_and_link() {
        let t = json!([{ "type": "tweet", "props": {
            "tweetId": "20", "tweetHandle": "jack", "author": "jack", "body": "just setting up\nmy twttr", "date": "2006年3月21日"
        }, "content": [] }]);
        assert_eq!(
            from_blocks(&t, &HashMap::new()),
            "> just setting up\n> my twttr\n> \n> — jack (@jack) 2006年3月21日\n\n[X の投稿](https://x.com/jack/status/20)\n"
        );
        // ID だけ（page post で取り込んだまま）ならリンクだけ
        let id_only = json!([{ "type": "tweet", "props": { "tweetId": "20" }, "content": [] }]);
        assert_eq!(from_blocks(&id_only, &HashMap::new()), "[X の投稿](https://x.com/i/status/20)\n");
    }

    #[test]
    fn youtube_as_watch_link() {
        let yt = json!([{ "type": "youtube", "props": { "videoId": "dQw4w9WgXcQ" }, "content": [] }]);
        assert_eq!(from_blocks(&yt, &HashMap::new()), "[YouTube](https://www.youtube.com/watch?v=dQw4w9WgXcQ)\n");
    }

    #[test]
    fn google_map_round_trip() {
        let blocks = to_blocks(
            "https://www.google.com/maps/d/viewer?mid=1AbCdEfGhIjKlMn\n\n\
             [地図](https://www.google.com/maps/place/%E6%9D%B1%E4%BA%AC%E9%A7%85/@35.68,139.76,17z)\n",
        );
        assert_eq!(blocks[0]["type"], "googleMap");
        assert_eq!(blocks[0]["props"]["mapsMid"], "1AbCdEfGhIjKlMn");
        assert_eq!(blocks[1]["props"]["mapsQ"], "東京駅");
        assert_eq!(
            from_blocks(&blocks, &HashMap::new()),
            "[Google マップ](https://www.google.com/maps/d/viewer?mid=1AbCdEfGhIjKlMn)\n\n\
             [Google マップ](https://www.google.com/maps?q=%E6%9D%B1%E4%BA%AC%E9%A7%85&ll=35.68,139.76&z=17)\n"
        );
    }

    #[test]
    fn bookmark_and_link_card_titles() {
        // bookmark はタイトルを props に持つ
        let bookmark = json!([{ "type": "bookmark", "props": { "url": "https://x", "title": "見積りフォーム" }, "content": [] }]);
        assert_eq!(from_blocks(&bookmark, &HashMap::new()), "[見積りフォーム](https://x)\n");
        // linkCard はタイトルを content（インライン）に持つ。props.title は無い
        let link_card = json!([{ "type": "linkCard", "props": { "url": "https://x" },
            "content": [{ "type": "text", "text": "見積りフォーム", "styles": {} }] }]);
        assert_eq!(from_blocks(&link_card, &HashMap::new()), "[見積りフォーム](https://x)\n");
        // タイトルが空なら URL にフォールバック
        let empty = json!([{ "type": "linkCard", "props": { "url": "https://x" }, "content": [] }]);
        assert_eq!(from_blocks(&empty, &HashMap::new()), "[https://x](https://x)\n");
    }
}
