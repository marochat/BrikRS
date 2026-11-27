//! YouTube の動画 ID（youtube ブロックの props.videoId）。フロントの youtube.ts と同じ規則

/// 英数字と - _ の11文字
pub fn is_video_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 動画の URL（watch・youtu.be・shorts・embed・live）から動画 ID を取り出す
pub fn video_id(raw: &str) -> Option<String> {
    let u = url::Url::parse(raw.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let mut path = u.path_segments()?;
    let id = match u.host_str()? {
        "youtu.be" => path.next()?.to_string(),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" | "www.youtube-nocookie.com" => {
            match path.next()? {
                "watch" => u.query_pairs().find(|(k, _)| k == "v")?.1.into_owned(),
                "shorts" | "embed" | "live" | "v" => path.next()?.to_string(),
                _ => return None,
            }
        }
        _ => return None,
    };
    is_video_id(&id).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_urls() {
        let id = Some("dQw4w9WgXcQ".to_string());
        assert_eq!(video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=10s"), id);
        assert_eq!(video_id("https://youtu.be/dQw4w9WgXcQ?si=abc"), id);
        assert_eq!(video_id("https://youtube.com/shorts/dQw4w9WgXcQ"), id);
        assert_eq!(video_id("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ"), id);
        assert_eq!(video_id("https://www.youtube.com/channel/UCxxxxxxxxx"), None);
        assert_eq!(video_id("https://example.com/watch?v=dQw4w9WgXcQ"), None);
        assert_eq!(video_id("https://www.youtube.com/watch?v=short"), None);
    }
}
