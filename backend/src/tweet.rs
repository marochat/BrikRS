//! X（旧 Twitter）の投稿の埋め込み（tweet ブロックの props）。フロントの tweet.ts と同じ規則。
//!
//! ブロックには挿入したときに X の oEmbed（publish.x.com/oembed、公開の API）から取った
//! 投稿者・本文・日付を保存して、BrikRS のカードとして出す（表示のたびに X へ取りに行かない）。
//! 公式の表示（画像・動画を含む）は、カードの「X で表示」を押したときに初めて
//! platform.twitter.com の iframe を読み込む（X の widgets.js は使わないので script-src は緩めない）。
//! iframe の URL は表示のたびに投稿の ID から組み立てる（入力された URL をそのまま src にしない）

use url::Url;

/// 投稿の ID：数字だけ（今の ID は19桁）
pub fn is_tweet_id(s: &str) -> bool {
    (1..=20).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// @名前（英数字と _ の15文字まで）
pub fn is_handle(s: &str) -> bool {
    (1..=15).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// 投稿の URL（x.com・twitter.com・mobile.・`/i/web/status/…`・`?s=20` など）から ID を取り出す。ID そのものも受け付ける
pub fn tweet_id(raw: &str) -> Option<String> {
    let s = raw.trim();
    if is_tweet_id(s) {
        return Some(s.to_string());
    }
    let u = Url::parse(s).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str()?;
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("mobile.")).unwrap_or(host);
    if !matches!(host, "x.com" | "twitter.com") {
        return None;
    }
    let segs: Vec<&str> = u.path_segments()?.filter(|p| !p.is_empty()).collect();
    // /<名前>/status/<ID>（後ろに /photo/1 などが付いてもよい）、/i/web/status/<ID>
    let id = match segs[..] {
        [_, "status" | "statuses", id, ..] => id,
        ["i", "web", "status", id, ..] => id,
        _ => return None,
    };
    is_tweet_id(id).then(|| id.to_string())
}

/// X の投稿のページ。@名前が分からなければ /i/status/<ID>（X が正しい名前へ転送する）
pub fn status_url(id: &str, handle: &str) -> String {
    if is_handle(handle) {
        format!("https://x.com/{handle}/status/{id}")
    } else {
        format!("https://x.com/i/status/{id}")
    }
}

/// oEmbed の html（`<blockquote class="twitter-tweet"><p>本文</p>&mdash; 名前 (@名前) <a>日付</a></blockquote>`）
/// から取り出したもの
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub body: String,
    pub date: String,
    /// 画像・動画が付いている（本文の pic.x.com のリンク。リンクそのものは本文から除く）
    pub media: bool,
}

/// 本文の上限（長文の投稿もあるので、ページの保存の上限に比べて十分小さく）
const MAX_BODY_CHARS: usize = 10_000;

pub fn parse_html(html: &str) -> Parsed {
    let mut out = Parsed::default();
    let Some(p_start) = html.find("<p") else { return out };
    let rest = &html[p_start..];
    let Some(open_end) = rest.find('>') else { return out };
    let (inner, after) = match rest.find("</p>") {
        Some(close) if close > open_end => (&rest[open_end + 1..close], &rest[close + 4..]),
        _ => return out,
    };
    let (body, media) = inline_text(inner);
    out.body = body.trim().chars().take(MAX_BODY_CHARS).collect();
    out.media = media;
    // 日付は本文の後ろの最後のリンクの文字
    if let Some(a) = after.rfind("<a") {
        let (date, _) = inline_text(&after[a..]);
        out.date = date.trim().chars().take(100).collect();
    }
    out
}

/// タグを除いて文字だけにする。`<br>` は改行、pic.x.com／pic.twitter.com のリンクは除いて media にする
fn inline_text(s: &str) -> (String, bool) {
    let mut text = String::new();
    let mut media = false;
    // 今いるリンクの文字（リンクの中だけ別に集めて、閉じたときに pic.… かを見る）
    let mut link: Option<String> = None;
    let mut rest = s;
    while let Some(lt) = rest.find('<') {
        let chunk = &rest[..lt];
        match link.as_mut() {
            Some(l) => l.push_str(chunk),
            None => text.push_str(chunk),
        }
        let Some(gt) = rest[lt..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[lt + 1..lt + gt].trim().to_ascii_lowercase();
        rest = &rest[lt + gt + 1..];
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        match (tag.starts_with('/'), name) {
            (false, "br") => match link.as_mut() {
                Some(l) => l.push('\n'),
                None => text.push('\n'),
            },
            (false, "a") => link = Some(String::new()),
            (true, "a") => {
                if let Some(l) = link.take() {
                    let shown = crate::ogp::decode_entities(&l);
                    if ["pic.x.com/", "pic.twitter.com/"].iter().any(|p| shown.trim().starts_with(p)) {
                        media = true;
                    } else {
                        text.push_str(&l);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(l) = link {
        text.push_str(&l);
    }
    text.push_str(rest);
    (crate::ogp::decode_entities(&text), media)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_urls() {
        let id = Some("1585341984679469056".to_string());
        assert_eq!(tweet_id("https://x.com/elonmusk/status/1585341984679469056"), id);
        assert_eq!(tweet_id("https://twitter.com/elonmusk/status/1585341984679469056?s=20&t=abc"), id);
        assert_eq!(tweet_id("https://mobile.twitter.com/a/status/1585341984679469056"), id);
        assert_eq!(tweet_id("https://www.x.com/a/status/1585341984679469056/photo/1"), id);
        assert_eq!(tweet_id("https://x.com/i/web/status/1585341984679469056"), id);
        assert_eq!(tweet_id("https://x.com/i/status/1585341984679469056"), id);
        assert_eq!(tweet_id(" 1585341984679469056 "), id);
        assert_eq!(tweet_id("https://x.com/elonmusk"), None);
        assert_eq!(tweet_id("https://example.com/a/status/1585341984679469056"), None);
        assert_eq!(tweet_id("https://x.com/a/status/12ab"), None);
        assert_eq!(tweet_id("javascript:alert(1)"), None);
    }

    #[test]
    fn builds_status_url() {
        assert_eq!(status_url("20", "jack"), "https://x.com/jack/status/20");
        assert_eq!(status_url("20", ""), "https://x.com/i/status/20");
        assert_eq!(status_url("20", "a/b"), "https://x.com/i/status/20");
    }

    #[test]
    fn parses_oembed_html() {
        let html = "<blockquote class=\"twitter-tweet\" data-lang=\"ja\"><p lang=\"en\" dir=\"ltr\">Entering &amp; HQ<br>2行目 \
            <a href=\"https://t.co/abc\">https://t.co/abc</a> \
            <a href=\"https://t.co/D68\">pic.twitter.com/D68</a></p>&mdash; Elon Musk (@elonmusk) \
            <a href=\"https://x.com/elonmusk/status/1?ref_src=twsrc%5Etfw\">2022年10月26日</a></blockquote>\n\n";
        let p = parse_html(html);
        assert_eq!(p.body, "Entering & HQ\n2行目 https://t.co/abc");
        assert_eq!(p.date, "2022年10月26日");
        assert!(p.media);

        let plain = parse_html("<blockquote><p lang=\"en\">just setting up my twttr</p>&mdash; jack (@jack) <a href=\"x\">2006年3月21日</a></blockquote>");
        assert_eq!(plain, Parsed { body: "just setting up my twttr".into(), date: "2006年3月21日".into(), media: false });
        assert_eq!(parse_html("no paragraph"), Parsed::default());
    }
}
