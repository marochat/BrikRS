//! X の投稿の埋め込み：投稿の URL から、X の oEmbed で投稿者・本文・日付を取ってきて返す（取得は ogp.rs、解析は tweet.rs）。
//! 内容は挿入するときにブロックへ保存する（表示のたびに X へ取りに行かない）

use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    auth::AuthUser,
    error::{AppError, AppResult},
    ogp::{self, FetchError},
    tweet,
};

/// oEmbed の応答の上限（本文は長くても数十KB）
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
pub struct TweetReq {
    url: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Tweet {
    id: String,
    /// @名前（@ は付けない）。読み取れなければ空
    handle: String,
    author: String,
    body: String,
    /// X が日本語で書いた日付（「2006年3月21日」など）
    date: String,
    media: bool,
}

pub async fn create(AuthUser(_): AuthUser, Json(req): Json<TweetReq>) -> AppResult<Json<Tweet>> {
    let id = tweet::tweet_id(&req.url)
        .ok_or_else(|| AppError::BadRequest("X の投稿の URL（https://x.com/…/status/…）として読み取れません".into()))?;
    Ok(Json(fetch(&id).await?))
}

async fn fetch(id: &str) -> AppResult<Tweet> {
    // @名前は入力の URL のものを使わない（違っていても ID だけで引ける）
    let mut api = Url::parse("https://publish.x.com/oembed").expect("固定の URL");
    api.query_pairs_mut()
        .append_pair("url", &format!("https://x.com/i/status/{id}"))
        .append_pair("omit_script", "1")
        .append_pair("dnt", "true")
        .append_pair("lang", "ja");
    let bytes = ogp::fetch_raw(api, "application/json", MAX_RESPONSE_BYTES)
        .await
        .map_err(|e| match e {
            FetchError::Rejected(m) => AppError::BadRequest(m.into()),
            // 消された投稿・鍵の掛かったアカウントは 404 などになる
            FetchError::Network(m) => AppError::BadRequest(format!(
                "投稿を読み込めませんでした（削除された投稿か、非公開のアカウントかもしれません）：{m}"
            )),
        })?;
    let v: Value = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::BadRequest("投稿を読み込めませんでした（応答の形式が違います）".into()))?;
    Ok(parse(id, &v))
}

fn parse(id: &str, v: &Value) -> Tweet {
    let author_url = v["author_url"].as_str().unwrap_or_default();
    let handle = Url::parse(author_url)
        .ok()
        .and_then(|u| u.path_segments()?.find(|s| !s.is_empty()).map(str::to_string))
        .filter(|h| tweet::is_handle(h))
        .unwrap_or_default();
    let p = tweet::parse_html(v["html"].as_str().unwrap_or_default());
    Tweet {
        id: id.to_string(),
        handle,
        author: v["author_name"].as_str().unwrap_or_default().chars().take(100).collect(),
        body: p.body,
        date: p.date,
        media: p.media,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 実際に X へ取りに行く（`cargo test -- --ignored`）
    #[tokio::test]
    #[ignore]
    async fn fetches_real_tweet() {
        let t = fetch("1585341984679469056").await.unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(t.handle, "elonmusk");
        assert!(t.body.starts_with("Entering Twitter HQ"), "{t:?}");
        assert!(t.media);
        assert!(t.date.contains("2022年"), "{t:?}");
    }

    #[test]
    fn parses_oembed_response() {
        let v = json!({
            "url": "https://x.com/jack/status/20",
            "author_name": "jack",
            "author_url": "https://x.com/jack",
            "html": "<blockquote class=\"twitter-tweet\"><p lang=\"en\" dir=\"ltr\">just setting up my twttr</p>&mdash; jack (@jack) <a href=\"https://x.com/jack/status/20?ref_src=twsrc%5Etfw\">2006年3月21日</a></blockquote>\n\n"
        });
        assert_eq!(
            parse("20", &v),
            Tweet {
                id: "20".into(),
                handle: "jack".into(),
                author: "jack".into(),
                body: "just setting up my twttr".into(),
                date: "2006年3月21日".into(),
                media: false,
            }
        );
        // 形の違う author_url は @名前なし
        let odd = json!({ "author_url": "https://x.com/a%2Fb", "html": "" });
        assert_eq!(parse("20", &odd).handle, "");
    }
}
