//! ゲストコメントのフォーム用トークンと、画像の CAPTCHA（外部サービスを使わず、このサーバーで作る）。
//!
//! トークン：`v1.<page_id>.<発行時刻>.<nonce>.<c|n>.<答えの HMAC か ->.<署名>`
//! - 答えそのものは入れない（鍵付きハッシュだけ）。鍵は起動時に作る乱数で、メモリにだけ置く
//!   （再起動で古いトークンは無効になるが、有効期間は15分だけなので困らない）
//! - 1つのトークンで送れるのは1回だけ。答えを間違えても使用済みにする（同じ画像で答えを総当たりさせない）
//! - 発行から3秒たたないうちの送信は断る（機械的な連投を減らす）
//! - 答えをその場で確かめる API は作らない
//!
//! portalspa の実装（captcha クレート、答えは HMAC だけ、使い捨て）を元にしている。

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::auth::random_token;

const TTL_SECS: i64 = 15 * 60;
const MIN_AGE_SECS: i64 = 3;
const CAPTCHA_LEN: u32 = 4;
/// 見分けにくい文字（大文字・小文字を区別しないので、I と l は形が同じに見える）
const CONFUSING: &[char] = &['I', 'l'];

type HmacSha256 = Hmac<Sha256>;

fn key() -> &'static [u8; 32] {
    static K: OnceLock<[u8; 32]> = OnceLock::new();
    K.get_or_init(|| {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).expect("OS RNG unavailable");
        k
    })
}

fn mac_hex(purpose: &str, data: &str) -> String {
    let mut m = HmacSha256::new_from_slice(key()).expect("HMAC accepts any key length");
    m.update(purpose.as_bytes());
    m.update(b"|");
    m.update(data.as_bytes());
    hex::encode(m.finalize().into_bytes())
}

fn normalize(answer: &str) -> String {
    answer.trim().to_uppercase()
}

fn answer_mac(nonce: &str, answer: &str) -> String {
    mac_hex("answer", &format!("{nonce}|{}", normalize(answer)))
}

fn used() -> &'static Mutex<HashMap<String, Instant>> {
    static U: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    U.get_or_init(Mutex::default)
}

/// 発行したトークンと、CAPTCHA の画像（PNG の base64。CAPTCHA を使わない設定なら None）
pub struct Issued {
    pub token: String,
    pub image_base64: Option<String>,
    pub expires_at: chrono::DateTime<Utc>,
}

/// トークンを発行する。with_captcha なら画像も作る（画像の生成は重いので別スレッドで）
pub async fn issue(page_id: i32, with_captcha: bool) -> anyhow::Result<Issued> {
    let (answer, image_base64) = if with_captcha {
        let (a, img) = tokio::task::spawn_blocking(render).await??;
        (Some(a), Some(img))
    } else {
        (None, None)
    };
    let now = Utc::now();
    Ok(Issued {
        token: make_token(page_id, now.timestamp(), &random_token()[..32], answer.as_deref()),
        image_base64,
        expires_at: now + chrono::Duration::seconds(TTL_SECS),
    })
}

fn make_token(page_id: i32, iat: i64, nonce: &str, answer: Option<&str>) -> String {
    let (flag, amac) = match answer {
        Some(a) => ("c", answer_mac(nonce, a)),
        None => ("n", "-".to_string()),
    };
    let body = format!("v1.{page_id}.{iat}.{nonce}.{flag}.{amac}");
    let sig = mac_hex("form", &body);
    format!("{body}.{sig}")
}

fn render() -> anyhow::Result<(String, String)> {
    use captcha::filters::{Dots, Noise, Wave};
    let mut c = captcha::Captcha::new();
    let letters: Vec<char> = c
        .supported_chars()
        .into_iter()
        .filter(|ch| ch.is_ascii_alphabetic() && !CONFUSING.contains(ch))
        .collect();
    // 画像の大きさとフィルタは portalspa で見比べて決めた値
    c.set_chars(&letters)
        .add_chars(CAPTCHA_LEN)
        .apply_filter(Noise::new(0.1))
        .apply_filter(Wave::new(2.0, 8.0))
        .view(160, 60)
        .apply_filter(Dots::new(4).max_radius(4).min_radius(2));
    let answer = c.chars_as_string();
    let img = c
        .as_base64()
        .ok_or_else(|| anyhow::anyhow!("CAPTCHA の画像を作れませんでした"))?;
    Ok((answer, img))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    /// 形が違う・署名が合わない・別のページのトークン
    Invalid,
    /// 発行から間もない（機械的な送信）
    TooFast,
    Expired,
    /// もう使われた
    Used,
    /// CAPTCHA の答えが違う
    WrongAnswer,
    /// CAPTCHA が必要になったのに、CAPTCHA の無いトークンだった（途中で設定が変わった）
    CaptchaRequired,
}

impl Reject {
    pub fn message(&self) -> &'static str {
        match self {
            Reject::Invalid | Reject::Expired | Reject::Used | Reject::CaptchaRequired => {
                "フォームの有効期限が切れました。画像を読み込み直してから、もう一度送ってください"
            }
            Reject::TooFast => "送信が早すぎます。少し待ってから、もう一度送ってください",
            Reject::WrongAnswer => "画像の文字が違います。新しい画像の文字を入力してください",
        }
    }
}

/// 送られてきたトークンを確かめ、使用済みにする
pub fn verify(token: &str, page_id: i32, answer: &str, require_captcha: bool) -> Result<(), Reject> {
    verify_at(token, page_id, answer, require_captcha, Utc::now().timestamp())
}

fn verify_at(token: &str, page_id: i32, answer: &str, require_captcha: bool, now: i64) -> Result<(), Reject> {
    let (body, sig) = token.rsplit_once('.').ok_or(Reject::Invalid)?;
    let mut m = HmacSha256::new_from_slice(key()).expect("HMAC accepts any key length");
    m.update(b"form|");
    m.update(body.as_bytes());
    let sig = hex::decode(sig).map_err(|_| Reject::Invalid)?;
    m.verify_slice(&sig).map_err(|_| Reject::Invalid)?;

    let parts: Vec<&str> = body.split('.').collect();
    let [ver, pid, iat, nonce, flag, amac] = parts[..] else {
        return Err(Reject::Invalid);
    };
    if ver != "v1" || pid.parse::<i32>().ok() != Some(page_id) {
        return Err(Reject::Invalid);
    }
    let iat: i64 = iat.parse().map_err(|_| Reject::Invalid)?;
    let age = now - iat;
    if age > TTL_SECS {
        return Err(Reject::Expired);
    }
    if age < MIN_AGE_SECS {
        // 早すぎる送信は使用済みにしない（少し待てば同じフォームで送れる）
        return Err(Reject::TooFast);
    }
    {
        let mut used = used().lock().unwrap();
        let ttl = Duration::from_secs(TTL_SECS as u64 + 60);
        used.retain(|_, t| t.elapsed() < ttl);
        if used.insert(nonce.to_string(), Instant::now()).is_some() {
            return Err(Reject::Used);
        }
    }
    match flag {
        "c" => {
            let expected = answer_mac(nonce, answer);
            if !crate::auth::constant_time_eq(expected.as_bytes(), amac.as_bytes()) {
                return Err(Reject::WrongAnswer);
            }
        }
        "n" if require_captcha => return Err(Reject::CaptchaRequired),
        "n" => {}
        _ => return Err(Reject::Invalid),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nonce(n: u8) -> String {
        format!("{n:032x}")
    }

    #[test]
    fn captcha_token_roundtrip() {
        let t = make_token(7, 1000, &nonce(1), Some("AbCd"));
        // 大文字・小文字は区別しない
        assert_eq!(verify_at(&t, 7, "abcd", true, 1010), Ok(()));
        // 使えるのは1回だけ
        assert_eq!(verify_at(&t, 7, "abcd", true, 1011), Err(Reject::Used));
    }

    #[test]
    fn wrong_answer_consumes_token() {
        let t = make_token(7, 1000, &nonce(2), Some("AbCd"));
        assert_eq!(verify_at(&t, 7, "xxxx", true, 1010), Err(Reject::WrongAnswer));
        assert_eq!(verify_at(&t, 7, "abcd", true, 1011), Err(Reject::Used));
    }

    #[test]
    fn timing_and_page_checks() {
        let t = make_token(7, 1000, &nonce(3), None);
        assert_eq!(verify_at(&t, 8, "", false, 1010), Err(Reject::Invalid));
        assert_eq!(verify_at(&t, 7, "", false, 1001), Err(Reject::TooFast));
        assert_eq!(verify_at(&t, 7, "", false, 1000 + TTL_SECS + 1), Err(Reject::Expired));
        // 早すぎた送信では使用済みにならない
        assert_eq!(verify_at(&t, 7, "", false, 1010), Ok(()));
    }

    #[test]
    fn captcha_required_and_tampering() {
        let t = make_token(7, 1000, &nonce(4), None);
        assert_eq!(verify_at(&t, 7, "", true, 1010), Err(Reject::CaptchaRequired));
        let t = make_token(7, 1000, &nonce(5), Some("AbCd"));
        // 署名の対象を書き換えると通らない
        let forged = t.replacen("v1.7.", "v1.8.", 1);
        assert_eq!(verify_at(&forged, 8, "abcd", true, 1010), Err(Reject::Invalid));
        assert_eq!(verify_at("garbage", 7, "", false, 1010), Err(Reject::Invalid));
    }

    #[test]
    fn renders_image() {
        let (answer, img) = render().unwrap();
        assert_eq!(answer.chars().count(), CAPTCHA_LEN as usize);
        assert!(!answer.contains(CONFUSING));
        assert!(!img.is_empty());
    }
}
