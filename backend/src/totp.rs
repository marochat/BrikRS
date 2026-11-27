//! 2段階認証（TOTP, RFC 6238）。Google Authenticator などの認証アプリで使う方式。
//!
//! 一般的な認証アプリに合わせて HMAC-SHA1・6桁・30秒にする。
//! 時計のずれを見込んで前後1区間（±30秒）まで受け付け、同じ区間のコードの使い回しは拒否する。

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

const STEP_SECS: i64 = 30;
const DIGITS: u32 = 6;
const SKEW_STEPS: i64 = 1;
const SECRET_BYTES: usize = 20;
const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn generate_secret() -> String {
    let mut buf = [0u8; SECRET_BYTES];
    getrandom::fill(&mut buf).expect("OS RNG unavailable");
    base32_encode(&buf)
}

fn base32_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !matches!(c, b'=' | b' ' | b'-')) {
        let v = BASE32.iter().position(|&x| x == c.to_ascii_uppercase())? as u32;
        acc = (acc << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn code_at(key: &[u8], step: i64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(&step.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[h.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    bin % 10u32.pow(DIGITS)
}

/// コードが合っていれば、その時間区間の番号を返す。
/// `last_step` 以前の区間は、既に使ったコードの使い回しになるので受け付けない
pub fn verify(secret: &str, code: &str, now_unix: i64, last_step: Option<i64>) -> Option<i64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let code: u32 = code.parse().ok()?;
    let key = base32_decode(secret)?;
    let now = now_unix.div_euclid(STEP_SECS);
    (now - SKEW_STEPS..=now + SKEW_STEPS)
        .filter(|s| last_step.is_none_or(|l| *s > l))
        .find(|&s| code_at(&key, s) == code)
}

/// 認証アプリに登録するための URI（QR コードの中身）
pub fn otpauth_uri(issuer: &str, account: &str, secret: &str) -> String {
    let label = format!("{}:{}", encode(issuer), encode(account));
    format!(
        "otpauth://totp/{label}?secret={secret}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        encode(issuer)
    )
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn qr_svg(data: &str) -> anyhow::Result<String> {
    let code = qrcode::QrCode::new(data.as_bytes())?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .quiet_zone(true)
        .build())
}

// ---------- リカバリーコード ----------

pub const RECOVERY_CODES: usize = 10;

/// 認証アプリを失くしたとき用の使い捨てコード（"xxxx-xxxx"、40bit）
pub fn generate_recovery_codes() -> Vec<String> {
    (0..RECOVERY_CODES)
        .map(|_| {
            let mut buf = [0u8; 5];
            getrandom::fill(&mut buf).expect("OS RNG unavailable");
            let s = base32_encode(&buf).to_lowercase();
            format!("{}-{}", &s[..4], &s[4..])
        })
        .collect()
}

/// 保存・照合用。入力の揺れ（大文字、ハイフンや空白の有無）を吸収してからハッシュにする
pub fn recovery_hash(code: &str) -> String {
    let norm: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    crate::auth::sha256_hex(&norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 付録 B の SHA1 のテストベクタ（8桁の下6桁）
    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn rfc6238_vectors() {
        let s = base32_encode(RFC_SECRET);
        for (t, code) in [
            (59, "287082"),
            (1111111109, "081804"),
            (1234567890, "005924"),
            (2000000000, "279037"),
        ] {
            assert_eq!(verify(&s, code, t, None), Some(t / 30), "t={t}");
        }
    }

    #[test]
    fn rejects_reuse_and_drift() {
        let s = base32_encode(RFC_SECRET);
        assert_eq!(
            verify(&s, "287082", 59, Some(1)),
            None,
            "同じ区間の使い回し"
        );
        assert_eq!(
            verify(&s, "287082", 59 + 30, None),
            Some(1),
            "1区間後は許容"
        );
        assert_eq!(verify(&s, "287082", 59 + 90, None), None, "3区間後は拒否");
        assert_eq!(verify(&s, "28708", 59, None), None);
        assert_eq!(verify(&s, "abcdef", 59, None), None);
    }

    #[test]
    fn base32_roundtrip() {
        let s = generate_secret();
        assert_eq!(s.len(), 32);
        assert_eq!(base32_decode(&s).unwrap().len(), SECRET_BYTES);
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
    }

    #[test]
    fn recovery_code_normalization() {
        let c = &generate_recovery_codes()[0];
        assert_eq!(
            recovery_hash(c),
            recovery_hash(&c.to_uppercase().replace('-', " "))
        );
    }
}
