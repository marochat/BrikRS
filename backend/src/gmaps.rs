//! Google マップの埋め込み（googleMap ブロックの props）。フロントの gmaps.ts と同じ規則。
//!
//! ブロックには次のどれかだけを保存し、iframe の URL は表示のたびに組み立てる（入力された URL をそのまま src にしない）。
//! - mapsPb：通常の地図の「地図を埋め込む」の `/maps/embed?pb=…` の pb（URL に使える文字だけ。エンコードされたまま持つ）
//! - mapsMid：マイマップの ID（`/maps/d/…?mid=…`）
//! - mapsQ（＋ mapsLl・mapsZoom）：場所の URL（`/maps/place/名前/@緯度,経度,17z` など）から取り出した場所の名前か座標。
//!   API キーの要らない `?q=…&output=embed` で表示する（Google が公開している方法ではない）

use url::Url;

/// googleMap ブロックの props の中身
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MapProps {
    pub pb: String,
    pub mid: String,
    pub q: String,
    pub ll: String,
    pub zoom: String,
}

const MAX_PB: usize = 4096;
const MAX_Q: usize = 200;

/// pb：`!` で始まり、URL のクエリにそのまま入れてよい文字だけ
pub fn is_pb(s: &str) -> bool {
    s.len() <= MAX_PB
        && s.starts_with('!')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!._~*%:+,-".contains(&b))
}

/// マイマップの ID
pub fn is_mid(s: &str) -> bool {
    (10..=100).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}

/// 場所の名前（組み立てるときにエンコードする）。制御文字は不可
pub fn is_q(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= MAX_Q && !s.chars().any(char::is_control)
}

/// 「緯度,経度」
pub fn is_ll(s: &str) -> bool {
    let Some((lat, lng)) = s.split_once(',') else { return false };
    let num = |t: &str, max: f64| {
        t.len() <= 20
            && t.bytes().all(|b| b.is_ascii_digit() || b == b'.' || b == b'-')
            && t.parse::<f64>().is_ok_and(|v| v.abs() <= max)
    };
    num(lat, 90.0) && num(lng, 180.0)
}

/// ズーム（1〜21）
pub fn is_zoom(s: &str) -> bool {
    s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u8>().is_ok_and(|z| (1..=21).contains(&z))
}

/// props のキーが地図のものなら、その値が正しい形か（地図のキーでなければ None）。空は常に可
pub fn check_prop(key: &str, s: &str) -> Option<bool> {
    let f: fn(&str) -> bool = match key {
        "mapsPb" => is_pb,
        "mapsMid" => is_mid,
        "mapsQ" => is_q,
        "mapsLl" => is_ll,
        "mapsZoom" => is_zoom,
        _ => return None,
    };
    Some(s.is_empty() || f(s))
}

fn is_google_host(h: &str) -> bool {
    matches!(
        h,
        "google.com" | "www.google.com" | "maps.google.com" | "google.co.jp" | "www.google.co.jp" | "maps.google.co.jp"
    )
}

/// パスの1区切り（`東京駅` や `Tokyo+Station` の % エンコード・+）を戻す
fn decode_segment(seg: &str) -> String {
    url::form_urlencoded::parse(format!("x={seg}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// `@35.681,139.767,17z` → （"35.681,139.767", "17"）。メートル指定（`1234m`、航空写真）はズームなし
fn parse_at(seg: &str) -> Option<(String, String)> {
    let mut it = seg.strip_prefix('@')?.split(',');
    let ll = format!("{},{}", it.next()?, it.next()?);
    if !is_ll(&ll) {
        return None;
    }
    let zoom = it
        .next()
        .and_then(|z| z.strip_suffix('z'))
        .and_then(|z| z.split('.').next())
        .filter(|z| is_zoom(z))
        .unwrap_or("")
        .to_string();
    Some((ll, zoom))
}

/// 地図の URL（埋め込みコードの src・マイマップ・場所の URL）を props にする。読み取れなければ None
pub fn parse(raw: &str) -> Option<MapProps> {
    let u = Url::parse(raw.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") || !is_google_host(u.host_str()?) {
        return None;
    }
    let host = u.host_str()?;
    let segs: Vec<&str> = u.path_segments()?.filter(|s| !s.is_empty()).collect();
    // `/maps/…` の後ろ（maps.google.com はパスの先頭に maps が無くてもよい）
    let rest: &[&str] = match segs.as_slice() {
        ["maps", rest @ ..] => rest,
        rest if host.starts_with("maps.") => rest,
        _ => return None,
    };
    let query = |key: &str| u.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v.into_owned());
    let mut m = MapProps::default();
    match rest {
        // 埋め込みコード（pb はデコードせずに持つ）
        ["embed"] => {
            let pb = u.query()?.split('&').find_map(|kv| kv.strip_prefix("pb="))?;
            m.pb = is_pb(pb).then(|| pb.to_string())?;
        }
        // マイマップ（embed・viewer・edit。`/maps/d/u/0/viewer` も）
        ["d", ..] => m.mid = query("mid").filter(|s| is_mid(s))?,
        // 経路・API キーの要る埋め込みなどは対象外
        ["dir" | "embed", ..] => return None,
        _ => {
            if let Some((ll, zoom)) = rest.iter().find_map(|s| parse_at(s)) {
                m.ll = ll;
                m.zoom = zoom;
            }
            let name = match rest {
                ["place" | "search", name, ..] if !name.starts_with('@') => Some(decode_segment(name)),
                _ => None,
            };
            m.q = match name.or_else(|| query("q")).or_else(|| query("query")).filter(|s| is_q(s)) {
                Some(q) => q.trim().to_string(),
                None if !m.ll.is_empty() => m.ll.clone(),
                None => return None,
            };
            if m.zoom.is_empty() {
                m.zoom = query("z").filter(|z| is_zoom(z)).unwrap_or_default();
            }
            if m.ll.is_empty() {
                m.ll = query("ll").filter(|s| is_ll(s)).unwrap_or_default();
            }
        }
    }
    Some(m)
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// ブラウザで開く URL（Markdown の書き出し用。読み直すと parse で同じ props に戻る）
pub fn open_url(m: &MapProps) -> Option<String> {
    if is_pb(&m.pb) {
        return Some(format!("https://www.google.com/maps/embed?pb={}", m.pb));
    }
    if is_mid(&m.mid) {
        return Some(format!("https://www.google.com/maps/d/viewer?mid={}", m.mid));
    }
    if !is_q(&m.q) {
        return None;
    }
    let mut s = format!("https://www.google.com/maps?q={}", enc(&m.q));
    if is_ll(&m.ll) {
        s.push_str(&format!("&ll={}", m.ll));
    }
    if is_zoom(&m.zoom) {
        s.push_str(&format!("&z={}", m.zoom));
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_embed_code_src() {
        let pb = "!1m18!1m12!1m3!1d3240.8!2d139.767!3d35.681!2m3!1f0!2f0!3f0!3m2!1i1024!2i768!4f13.1!3m3!1m2!1s0x60188bfbd89f700b%3A0x277c49ba34ed38!2z5p2x5Lqs6aeF!5e0!3m2!1sja!2sjp!4v1700000000000!5m2!1sja!2sjp";
        let m = parse(&format!("https://www.google.com/maps/embed?pb={pb}")).unwrap();
        assert_eq!(m.pb, pb);
        assert_eq!(parse("https://www.google.com/maps/embed?pb=!1m2;x"), None);
        assert_eq!(parse("https://www.google.com/maps/embed?pb=1m2"), None);
        assert_eq!(parse("https://www.google.com/maps/embed/v1/place?key=K&q=a"), None);
    }

    #[test]
    fn parses_my_maps() {
        let mid = "1AbCdEfGhIjKlMnOpQrStUvWxYz012345";
        for u in [
            format!("https://www.google.com/maps/d/embed?mid={mid}&ehbc=2E312F"),
            format!("https://www.google.com/maps/d/viewer?mid={mid}&ll=35.6,139.7&z=10"),
            format!("https://www.google.com/maps/d/u/0/edit?mid={mid}"),
        ] {
            assert_eq!(parse(&u).unwrap().mid, mid, "{u}");
        }
        assert_eq!(parse("https://www.google.com/maps/d/viewer?mid=a<b"), None);
    }

    #[test]
    fn parses_place_urls() {
        let m = parse("https://www.google.com/maps/place/%E6%9D%B1%E4%BA%AC%E9%A7%85/@35.6812362,139.7645445,17z/data=!3m1!4b1").unwrap();
        assert_eq!((m.q.as_str(), m.ll.as_str(), m.zoom.as_str()), ("東京駅", "35.6812362,139.7645445", "17"));
        let m = parse("https://www.google.co.jp/maps/place/Tokyo+Station/@35.68,139.76,15.5z").unwrap();
        assert_eq!((m.q.as_str(), m.zoom.as_str()), ("Tokyo Station", "15"));
        let m = parse("https://www.google.com/maps/@35.68,139.76,1234m/data=!3m1!1e3").unwrap();
        assert_eq!((m.q.as_str(), m.zoom.as_str()), ("35.68,139.76", ""));
        let m = parse("https://maps.google.com/maps?q=%E6%9C%AD%E5%B9%8C&z=12").unwrap();
        assert_eq!((m.q.as_str(), m.zoom.as_str()), ("札幌", "12"));
        let m = parse("https://www.google.com/maps/search/?api=1&query=%E4%BA%AC%E9%83%BD%E9%A7%85").unwrap();
        assert_eq!(m.q, "京都駅");
        assert_eq!(parse("https://www.google.com/maps/dir/A/B"), None);
        assert_eq!(parse("https://www.google.com/search?q=a"), None);
        assert_eq!(parse("https://example.com/maps/place/x/@35,139,10z"), None);
        assert_eq!(parse("https://maps.app.goo.gl/abc"), None);
    }

    #[test]
    fn open_url_round_trips() {
        for u in [
            "https://www.google.com/maps/embed?pb=!1m3!1d3240.8",
            "https://www.google.com/maps/d/viewer?mid=1AbCdEfGhIjKlMn",
            "https://www.google.com/maps/place/%E6%9D%B1%E4%BA%AC%E9%A7%85/@35.68,139.76,17z",
        ] {
            let m = parse(u).unwrap();
            assert_eq!(parse(&open_url(&m).unwrap()).unwrap(), m, "{u}");
        }
    }

    #[test]
    fn checks_props() {
        assert_eq!(check_prop("mapsZoom", "22"), Some(false));
        assert_eq!(check_prop("mapsLl", "91,0"), Some(false));
        assert_eq!(check_prop("mapsQ", ""), Some(true));
        assert_eq!(check_prop("mapsPb", "!1m2\"><script>"), Some(false));
        assert_eq!(check_prop("title", "x"), None);
    }
}
