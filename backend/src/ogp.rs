//! Web ブックマーク（リンクカード）用に、外部のページから OGP（タイトル・説明・画像）を取ってくる。
//!
//! 入力された URL へサーバーから接続するので、内部のネットワーク（DB やルーター、クラウドのメタデータ）を
//! 覗く踏み台にされないようにする（SSRF 対策）：
//! - http / https の 80・443 番だけ
//! - 名前解決した IP がすべてグローバルなアドレスであることを確かめ、その IP に固定して接続する（DNS の差し替え対策）
//! - リダイレクトは自分で追い、行き先ごとに同じ検査をする
//! - システムのプロキシ設定は使わない（プロキシ経由だと検査をすり抜ける）
//! - 時間とサイズに上限
//! - このサーバー自身のグローバルなアドレスにも接続しない（OGP_DENY_IPS、既定は PUBLIC_URL のホスト）。
//!   コンテナから自分のグローバル IP へつなぐと、リバースプロキシには Docker の内部 IP からの接続として届くので、
//!   プロキシに「内部 IP なら許可」の設定があると、そこを覗けてしまう

use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

use reqwest::{
    Client, Response, StatusCode, header,
    dns::{Addrs, Name, Resolve, Resolving},
};
use url::{Host, Url};

const MAX_REDIRECTS: usize = 5;
/// 1回の接続ごと（リダイレクトのたびに数え直す）
const TIMEOUT: Duration = Duration::from_secs(15);
/// fetch / fetch_image 1回あたりの全体の上限（リダイレクトをすべて合わせて）
const TOTAL_TIMEOUT: Duration = Duration::from_secs(25);
/// OGP は <head> にあるので、<body> が出てきたらそこで読むのをやめる。
/// <head> にスクリプトを大量に埋め込むサイト（YouTube は 700KB 以上）もあるので上限は大きめに
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const TITLE_MAX_CHARS: usize = 200;
const DESCRIPTION_MAX_CHARS: usize = 300;
const USER_AGENT: &str = concat!("BrikRS/", env!("CARGO_PKG_VERSION"), " (link preview)");

#[derive(Debug, Default, PartialEq)]
pub struct Ogp {
    pub title: String,
    pub description: String,
    pub site_name: String,
    /// 画像の絶対 URL（ページの URL を基準に解決したもの）
    pub image: Option<Url>,
}

#[derive(Debug)]
pub enum FetchError {
    /// 利用者に見せてよい理由
    Rejected(&'static str),
    Network(String),
}

type Result<T> = std::result::Result<T, FetchError>;

/// 入力された URL を検査して整える（http / https のみ、80・443 番のみ）
pub fn parse_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw.trim()).map_err(|_| FetchError::Rejected("URL の形式が正しくありません"))?;
    check_url(&url)?;
    Ok(url)
}

fn check_url(url: &Url) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::Rejected("http / https の URL だけ使えます"));
    }
    if !matches!(url.port_or_known_default(), Some(80 | 443)) {
        return Err(FetchError::Rejected("80・443 番以外のポートには接続しません"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::Rejected("ユーザー名・パスワード付きの URL は使えません"));
    }
    Ok(())
}

/// グローバルなアドレスか（プライベート・ループバック・リンクローカル・予約済みなどを除く）
fn is_global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_global_v4(v4),
        IpAddr::V6(v6) => {
            // IPv4 射影アドレス（::ffff:a.b.c.d）は中の IPv4 で判定する
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_global_v4(v4);
            }
            let s = v6.segments();
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // ユニークローカル fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // リンクローカル fe80::/10
                || (s[0] & 0xffc0) == 0xfec0 // サイトローカル（廃止） fec0::/10
                || (s[0] == 0x2001 && s[1] == 0x0db8) // 文書用 2001:db8::/32
                || (s[0] == 0x0064 && s[1] == 0xff9b) // NAT64 64:ff9b::/96（中身が内部の IPv4 になりうる）
                || s[0] == 0x2002 // 6to4
                || s[..6] == [0; 6]) // ::/96（IPv4 互換など）
        }
    }
}

fn is_global_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (b & 0xc0) == 64) // 共有アドレス 100.64.0.0/10
        || (a == 192 && b == 0 && c == 0) // 192.0.0.0/24
        || (a == 198 && (b & 0xfe) == 18) // ベンチマーク用 198.18.0.0/15
        || a >= 240) // 予約済み 240.0.0.0/4
}

// ---------- このサーバー自身のアドレス（接続を拒否する宛先） ----------

/// 接続を拒否する宛先の1項目
#[derive(Debug, PartialEq)]
enum Deny {
    /// IP アドレスか CIDR（ネットワークのアドレス、プレフィックス長）
    Net(IpAddr, u8),
    /// ホスト名（取得のたびに名前解決する。IP が変わっても追従する）
    Host(String),
}

/// OGP_DENY_IPS（カンマか空白区切り）を読む。未設定なら PUBLIC_URL のホスト。空文字なら拒否しない
fn deny_entries() -> &'static [Deny] {
    static LIST: OnceLock<Vec<Deny>> = OnceLock::new();
    LIST.get_or_init(|| {
        let raw = std::env::var("OGP_DENY_IPS").ok().unwrap_or_else(|| {
            std::env::var("PUBLIC_URL")
                .ok()
                .and_then(|u| Url::parse(&u).ok())
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_default()
        });
        parse_deny(&raw)
    })
}

fn parse_deny(raw: &str) -> Vec<Deny> {
    raw.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| {
            let (addr, prefix) = match s.split_once('/') {
                Some((a, p)) => (a, Some(p)),
                None => (s, None),
            };
            // IPv6 は URL と同じ [..] の書き方も受け付ける
            let addr = addr.trim_start_matches('[').trim_end_matches(']');
            match addr.parse::<IpAddr>() {
                Ok(ip) => {
                    let ip = ip.to_canonical();
                    let max = if ip.is_ipv4() { 32 } else { 128 };
                    let len = match prefix {
                        Some(p) => match p.parse::<u8>() {
                            Ok(n) if n <= max => n,
                            _ => {
                                tracing::warn!("OGP_DENY_IPS の {s} は読めないので無視します");
                                return None;
                            }
                        },
                        None => max,
                    };
                    Some(Deny::Net(ip, len))
                }
                Err(_) if prefix.is_none() => Some(Deny::Host(s.to_ascii_lowercase())),
                Err(_) => {
                    tracing::warn!("OGP_DENY_IPS の {s} は読めないので無視します");
                    None
                }
            }
        })
        .collect()
}

fn in_net(ip: IpAddr, net: IpAddr, len: u8) -> bool {
    match (ip.to_canonical(), net) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let mask = u32::MAX.checked_shl(32 - len as u32).unwrap_or(0);
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let mask = u128::MAX.checked_shl(128 - len as u32).unwrap_or(0);
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}

/// 接続を拒否する宛先（ホスト名は今の名前解決の結果に展開する）。
/// 名前解決に失敗したホスト名は飛ばす（このサーバーの DNS が引けないときにブックマーク全体を止めない）
async fn denied_nets() -> Vec<(IpAddr, u8)> {
    let mut out = Vec::new();
    for d in deny_entries() {
        match d {
            Deny::Net(ip, len) => out.push((*ip, *len)),
            Deny::Host(h) => match tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((h.as_str(), 0))).await {
                Ok(Ok(addrs)) => out.extend(addrs.map(|a| {
                    let ip = a.ip().to_canonical();
                    (ip, if ip.is_ipv4() { 32 } else { 128 })
                })),
                _ => tracing::warn!("OGP_DENY_IPS のホスト {h} を名前解決できません（今回は拒否の対象から外します）"),
            },
        }
    }
    out
}

/// 接続先のアドレスとして使えるか：グローバルで、このサーバー自身のアドレスでもない
fn check_addrs(addrs: &[SocketAddr], denied: &[(IpAddr, u8)]) -> Result<()> {
    if addrs.is_empty() || !addrs.iter().all(|a| is_global(a.ip())) {
        return Err(FetchError::Rejected("内部のネットワークのアドレスには接続しません"));
    }
    if addrs
        .iter()
        .any(|a| denied.iter().any(|&(n, len)| in_net(a.ip(), n, len)))
    {
        return Err(FetchError::Rejected("このサーバー自身のアドレスには接続しません"));
    }
    Ok(())
}

/// 起動時に、拒否する宛先の設定をログに出す
pub fn log_deny_config() {
    let list = deny_entries();
    if list.is_empty() {
        tracing::info!("Web ブックマーク：このサーバー自身のアドレスの拒否は無効（OGP_DENY_IPS・PUBLIC_URL が未設定）");
    } else {
        tracing::info!("Web ブックマーク：接続を拒否する宛先 {list:?}");
    }
}

/// 名前解決して、すべてのアドレスがグローバル（かつこのサーバー自身ではない）なら接続先を1つ返す
async fn resolve(url: &Url) -> Result<SocketAddr> {
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = match url.host() {
        Some(Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(Host::Domain(d)) => tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((d, port)))
            .await
            .map_err(|_| FetchError::Network("名前解決がタイムアウトしました".into()))?
            .map_err(|_| FetchError::Rejected("ホスト名が見つかりません"))?
            .collect(),
        None => return Err(FetchError::Rejected("URL にホスト名がありません")),
    };
    // 1つでも内部のアドレスが混じっていれば断る（どれに接続されるかを相手に選ばせない）
    check_addrs(&addrs, &denied_nets().await)?;
    Ok(addrs[0])
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("rustls の既定のプロトコル")
            .with_root_certificates(roots)
            .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// 共有クライアント用の DNS 解決。ホスト名がグローバルな IP にしか解決できない場合だけ返す。
/// ホストが最初から IP リテラルの場合は hyper がこれを呼ばずに直接つなぐので、その検査は
/// （こちらより前に必ず通る）呼び出し側の [`resolve`] が担う
struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            match check_addrs(&addrs, &denied_nets().await) {
                Ok(()) => {}
                Err(FetchError::Rejected(m)) => return Err(m.into()),
                Err(FetchError::Network(m)) => return Err(m.into()),
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// すべての hop・すべてのリクエストで共有する1つのクライアント（hop ごとに作り直さない）
fn client() -> Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .tls_backend_preconfigured((*tls_config()).clone())
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(TIMEOUT)
                .user_agent(USER_AGENT)
                .dns_resolver(SafeResolver)
                .build()
                .expect("reqwest クライアントの構築")
        })
        .clone()
}

/// 1回だけ GET する（リダイレクトは追わない）。接続先は事前に検査し、実際の解決・接続は
/// 共有クライアントの [`SafeResolver`] がもう一度検査してから行う
async fn get_once(url: &Url, accept: &'static str) -> Result<Response> {
    resolve(url).await?;
    client()
        .get(url.clone())
        .header(header::ACCEPT, accept)
        .header(header::ACCEPT_LANGUAGE, "ja,en;q=0.8")
        .send()
        .await
        .map_err(|e| FetchError::Network(if e.is_timeout() { "タイムアウトしました".into() } else { format!("{e}") }))
}

/// リダイレクトを追いながら GET する。最終的な URL と応答を返す
async fn get(url: Url, accept: &'static str) -> Result<(Url, Response)> {
    let mut url = url;
    for _ in 0..=MAX_REDIRECTS {
        let resp = get_once(&url, accept).await?;
        if !resp.status().is_redirection() {
            if resp.status() != StatusCode::OK {
                return Err(FetchError::Network(format!("ページを取得できませんでした（{}）", resp.status())));
            }
            return Ok((url, resp));
        }
        let next = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|loc| url.join(loc).ok())
            .ok_or(FetchError::Network("リダイレクト先が不正です".into()))?;
        check_url(&next)?;
        url = next;
    }
    Err(FetchError::Network("リダイレクトが多すぎます".into()))
}

/// 本文を上限まで読む。truncate なら上限で打ち切り、そうでなければ上限を超えたら失敗にする。
/// until_body なら <body が出てきたところで読むのをやめる（HTML 用）
async fn read_body(mut resp: Response, limit: usize, truncate: bool, until_body: bool) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| FetchError::Network(format!("{e}")))?
    {
        // 前の塊との境目にまたがる "<body" も見つけられるよう、少し前から探す
        let from = buf.len().saturating_sub(4);
        buf.extend_from_slice(&chunk);
        if until_body && buf[from..].windows(5).any(|w| w.eq_ignore_ascii_case(b"<body")) {
            break;
        }
        if buf.len() > limit {
            if truncate {
                buf.truncate(limit);
                break;
            }
            return Err(FetchError::Rejected("ファイルが大きすぎます"));
        }
    }
    Ok(buf)
}

/// ページを取得して OGP を読む。戻り値の Url はリダイレクト後の URL
pub async fn fetch(url: Url) -> Result<(Url, Ogp)> {
    with_total_timeout(fetch_inner(url)).await
}

async fn fetch_inner(url: Url) -> Result<(Url, Ogp)> {
    let (final_url, resp) = get(url, "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1").await?;
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.is_empty() && !content_type.contains("html") {
        return Err(FetchError::Rejected("HTML のページではありません"));
    }
    let bytes = read_body(resp, MAX_HTML_BYTES, true, true).await?;
    let html = decode(&bytes, &content_type);
    Ok((final_url.clone(), parse(&html, &final_url)))
}

/// 画像を取得する（形式の確認は呼び出し側で先頭のバイト列から行う）
pub async fn fetch_image(url: Url) -> Result<Vec<u8>> {
    with_total_timeout(fetch_image_inner(url)).await
}

async fn fetch_image_inner(url: Url) -> Result<Vec<u8>> {
    check_url(&url)?;
    let (_, resp) = get(url, "image/avif,image/webp,image/png,image/jpeg,image/gif;q=0.9,*/*;q=0.1").await?;
    read_body(resp, MAX_IMAGE_BYTES, false, false).await
}

/// API などの応答の本文をそのまま取得する（limit を超えたら失敗。Gist の取得に使う）
pub async fn fetch_raw(url: Url, accept: &'static str, limit: usize) -> Result<Vec<u8>> {
    with_total_timeout(async move {
        check_url(&url)?;
        let (_, resp) = get(url, accept).await?;
        read_body(resp, limit, false, false).await
    })
    .await
}

/// リダイレクトの追跡・読み込みをすべて合わせて、全体に上限を掛ける
async fn with_total_timeout<T>(fut: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(TOTAL_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| Err(FetchError::Network("取得がタイムアウトしました".into())))
}

// ---------- HTML の解析 ----------

/// 文字コード：Content-Type の charset → <meta charset> → UTF-8 の順
fn decode(bytes: &[u8], content_type: &str) -> String {
    let label = charset_param(content_type).or_else(|| {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
        let pos = head.find("charset=")?;
        let rest = head[pos + 8..].trim_start_matches(['"', '\'']);
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    });
    let enc = label
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    enc.decode(bytes).0.into_owned()
}

fn charset_param(content_type: &str) -> Option<String> {
    let pos = content_type.find("charset=")?;
    Some(
        content_type[pos + 8..]
            .trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace())
            .split(';')
            .next()?
            .trim_matches(['"', '\''])
            .to_string(),
    )
}

/// <meta> と <title> から OGP を拾う（og: → twitter: → 通常の description / <title> の順に使う）
pub fn parse(html: &str, base: &Url) -> Ogp {
    // ASCII だけを小文字にするのでバイト位置は html と同じ
    let lower = html.to_ascii_lowercase();
    // <body> より後ろは見ない（OGP は <head> にある）
    let head_end = lower.find("<body").unwrap_or(lower.len());

    let mut metas: Vec<(String, String)> = Vec::new();
    let mut pos = 0;
    while let Some(i) = lower[pos..head_end].find("<meta") {
        let start = pos + i;
        let Some(end) = find_tag_end(&lower[start..]) else { break };
        let attrs = parse_attrs(&html[start + 5..start + end]);
        let key = attrs
            .iter()
            .find(|(k, _)| k == "property" || k == "name")
            .map(|(_, v)| v.to_ascii_lowercase());
        let content = attrs.iter().find(|(k, _)| k == "content").map(|(_, v)| v.clone());
        if let (Some(k), Some(c)) = (key, content) {
            metas.push((k, c));
        }
        pos = (start + end).min(head_end);
    }
    let get = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|k| metas.iter().find(|(mk, v)| mk == k && !v.trim().is_empty()))
            .map(|(_, v)| clean(v))
            .unwrap_or_default()
    };

    let mut title = get(&["og:title", "twitter:title"]);
    if title.is_empty() {
        if let Some(i) = lower.find("<title") {
            if let Some(gt) = lower[i..].find('>') {
                let s = i + gt + 1;
                if let Some(e) = lower[s..].find("</title") {
                    title = clean(&decode_entities(&html[s..s + e]));
                }
            }
        }
    }
    let image = get(&["og:image:secure_url", "og:image", "og:image:url", "twitter:image", "twitter:image:src"]);
    Ogp {
        title: truncate(&title, TITLE_MAX_CHARS),
        description: truncate(&get(&["og:description", "twitter:description", "description"]), DESCRIPTION_MAX_CHARS),
        site_name: truncate(&get(&["og:site_name"]), TITLE_MAX_CHARS),
        image: (!image.is_empty())
            .then(|| base.join(&image).ok())
            .flatten()
            .filter(|u| matches!(u.scheme(), "http" | "https")),
    }
}

/// タグを閉じる `>` までの相対位置を返す（クォートされた属性値の中の `>` は無視する）
fn find_tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '>' => return Some(i),
                _ => {}
            },
        }
    }
    None
}

/// タグの中身（`<meta` の後ろから `>` の手前まで）の属性を読む。値の文字参照は戻す
fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() || c == '/' {
            chars.next();
            continue;
        }
        // 属性名
        let mut end = i;
        while let Some(&(j, c)) = chars.peek() {
            if c.is_whitespace() || c == '=' || c == '/' {
                break;
            }
            end = j + c.len_utf8();
            chars.next();
        }
        let name = s[i..end].to_ascii_lowercase();
        while chars.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
            chars.next();
        }
        let mut value = String::new();
        if chars.peek().is_some_and(|&(_, c)| c == '=') {
            chars.next();
            while chars.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
                chars.next();
            }
            match chars.peek().map(|&(_, c)| c) {
                Some(q @ ('"' | '\'')) => {
                    chars.next();
                    for (_, c) in chars.by_ref() {
                        if c == q {
                            break;
                        }
                        value.push(c);
                    }
                }
                _ => {
                    while let Some(&(_, c)) = chars.peek() {
                        if c.is_whitespace() {
                            break;
                        }
                        value.push(c);
                        chars.next();
                    }
                }
            }
        }
        if !name.is_empty() {
            out.push((name, decode_entities(&value)));
        }
    }
    out
}

pub(crate) fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // 文字参照は短いので、先頭の数バイトだけで ';' を探す（バイト単位で探すので日本語の途中で切れない）
        let Some(semi) = rest.bytes().take(12).position(|b| b == b';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 改行や連続する空白を1つにまとめ、制御文字を落とす
fn clean(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max).collect();
    t.push('…');
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_internal_addresses() {
        for ip in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "0.0.0.0",
            "100.64.0.1", "198.18.0.1", "240.0.0.1", "255.255.255.255", "::1", "::", "fc00::1",
            "fd12::1", "fe80::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1", "64:ff9b::a00:1", "2001:db8::1",
        ] {
            assert!(!is_global(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111", "::ffff:8.8.8.8"] {
            assert!(is_global(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn parses_deny_list() {
        let d = parse_deny("203.0.113.5, 198.51.100.0/24 [2001:db8::1] 2001:db8:1::/48 wiki.example.org 1.2.3.4/33");
        assert_eq!(
            d,
            vec![
                Deny::Net("203.0.113.5".parse().unwrap(), 32),
                Deny::Net("198.51.100.0".parse().unwrap(), 24),
                Deny::Net("2001:db8::1".parse().unwrap(), 128),
                Deny::Net("2001:db8:1::".parse().unwrap(), 48),
                Deny::Host("wiki.example.org".into()),
            ],
            "読めない 1.2.3.4/33 は捨てる"
        );
        assert!(parse_deny("").is_empty());
        // IPv4 射影アドレスで書いても IPv4 として扱う
        assert_eq!(parse_deny("::ffff:203.0.113.5"), vec![Deny::Net("203.0.113.5".parse().unwrap(), 32)]);
    }

    #[test]
    fn rejects_own_address() {
        let ip = |s: &str| -> IpAddr { s.parse().unwrap() };
        let sa = |s: &str| SocketAddr::new(ip(s), 443);
        let denied = vec![(ip("8.8.4.4"), 32), (ip("1.1.1.0"), 24), (ip("2606:4700::"), 32)];
        assert!(check_addrs(&[sa("8.8.8.8")], &denied).is_ok());
        assert!(check_addrs(&[sa("8.8.4.4")], &denied).is_err());
        assert!(check_addrs(&[sa("1.1.1.1")], &denied).is_err());
        assert!(check_addrs(&[sa("1.1.2.1")], &denied).is_ok());
        assert!(check_addrs(&[sa("2606:4700:4700::1111")], &denied).is_err());
        // 射影アドレスで来ても IPv4 の一覧で止める
        assert!(check_addrs(&[sa("::ffff:8.8.4.4")], &denied).is_err());
        // 1つでも当たれば断る
        assert!(check_addrs(&[sa("8.8.8.8"), sa("8.8.4.4")], &denied).is_err());
        // /0 は全部
        assert!(check_addrs(&[sa("9.9.9.9")], &[(ip("0.0.0.0"), 0)]).is_err());
        // 内部のアドレスは一覧が空でも断る
        assert!(check_addrs(&[sa("10.0.0.1")], &[]).is_err());
    }

    #[test]
    fn checks_urls() {
        assert!(parse_url("https://example.com/a").is_ok());
        assert!(parse_url("http://example.com:80/").is_ok());
        assert!(parse_url("ftp://example.com/").is_err());
        assert!(parse_url("file:///etc/passwd").is_err());
        assert!(parse_url("http://example.com:8080/").is_err());
        assert!(parse_url("http://user:pw@example.com/").is_err());
        assert!(parse_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn parses_ogp() {
        let html = r#"<!doctype html><html><head>
            <title>普通のタイトル</title>
            <meta property="og:title" content="OGP の &quot;タイトル&quot; &amp; 記号">
            <meta name='description' content='説明（description）'>
            <meta content="サイト名" property="og:site_name" />
            <meta property=og:image content=/img/card.png>
            </head><body><meta property="og:description" content="body の中は見ない"></body></html>"#;
        let base = Url::parse("https://example.com/articles/1").unwrap();
        let o = parse(html, &base);
        assert_eq!(o.title, "OGP の \"タイトル\" & 記号");
        assert_eq!(o.description, "説明（description）");
        assert_eq!(o.site_name, "サイト名");
        assert_eq!(o.image.unwrap().as_str(), "https://example.com/img/card.png");
    }

    #[test]
    fn quoted_attribute_can_contain_literal_gt() {
        // 属性値の中にエスケープされていない '>' があっても、タグの終端検出が誤爆しないことを確認する
        let html = r#"<html><head>
            <meta property="og:title" content="A > B ランキング">
            </head></html>"#;
        let base = Url::parse("https://example.com/").unwrap();
        let o = parse(html, &base);
        assert_eq!(o.title, "A > B ランキング");
    }

    #[test]
    fn falls_back_to_title_tag() {
        let base = Url::parse("https://example.com/").unwrap();
        let o = parse("<html><head><title>\n  タグの\n  タイトル &#x2764; </title></head></html>", &base);
        assert_eq!(o.title, "タグの タイトル ❤");
        assert_eq!(o.image, None);
    }

    #[test]
    fn decodes_entities_next_to_multibyte_text() {
        // ';' を探す範囲が日本語の文字の途中で切れても落ちない
        assert_eq!(decode_entities("A&B 公式サイト"), "A&B 公式サイト");
        assert_eq!(decode_entities("&#12354;&amp;公"), "あ&公");
    }

    #[test]
    fn decodes_shift_jis() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("<meta charset=\"Shift_JIS\"><title>日本語</title>");
        assert!(decode(&bytes, "text/html").contains("日本語"));
        let (bytes, _, _) = encoding_rs::EUC_JP.encode("<title>日本語</title>");
        assert!(decode(&bytes, "text/html; charset=EUC-JP").contains("日本語"));
    }

    /// 実際のサイトから取ってくる（ネットワークが要るので `cargo test -- --ignored` のときだけ）
    #[tokio::test]
    #[ignore]
    async fn fetches_real_pages() {
        let (_, o) = fetch(parse_url("https://example.com/").unwrap()).await.unwrap();
        assert_eq!(o.title, "Example Domain");
        // 内部のアドレスは断る
        assert!(matches!(
            fetch(parse_url("http://localhost/").unwrap()).await,
            Err(FetchError::Rejected(_))
        ));
    }

    /// OGP_DENY_IPS にホスト名を書くと、そのホストの今の IP へは接続しない
    /// `OGP_DENY_IPS=example.com cargo test denies_configured_host -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn denies_configured_host() {
        assert_eq!(std::env::var("OGP_DENY_IPS").as_deref(), Ok("example.com"), "OGP_DENY_IPS=example.com で実行する");
        assert!(matches!(
            fetch(parse_url("https://example.com/").unwrap()).await,
            Err(FetchError::Rejected("このサーバー自身のアドレスには接続しません"))
        ));
        // 他のホストには影響しない
        assert!(fetch(parse_url("https://www.iana.org/").unwrap()).await.is_ok());
    }

    /// 環境変数 OGP_URLS（空白区切り）の URL を取得して結果を表示する（取れないサイトを調べる用）
    /// `OGP_URLS="https://..." cargo test show_ogp -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn show_ogp() {
        for u in std::env::var("OGP_URLS").unwrap_or_default().split_whitespace() {
            match fetch(parse_url(u).unwrap()).await {
                Ok((f, o)) => println!("{u}\n  -> {f}\n  {o:?}"),
                Err(e) => println!("{u}\n  !! {e:?}"),
            }
        }
    }
}
