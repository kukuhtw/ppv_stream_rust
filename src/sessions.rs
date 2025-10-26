// src/sessions.rs
// src/sessions.rs
use chrono::{Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;

use crate::config::Config;
use base64ct::{Base64UrlUnpadded, Encoding};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const COOKIE_NAME: &str = "ppv_session";

fn b64(s: &str) -> String {
    Base64UrlUnpadded::encode_string(s.as_bytes())
}
fn b64_bytes(bytes: &[u8]) -> String {
    Base64UrlUnpadded::encode_string(bytes)
}
fn b64_decode_to_string(s: &str) -> Option<String> {
    let bytes = Base64UrlUnpadded::decode_vec(s).ok()?;
    String::from_utf8(bytes).ok()
}


fn sign_sid(sid: &str, secret: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC key");
    mac.update(sid.as_bytes());
    let tag = mac.finalize().into_bytes();
    b64_bytes(&tag)
}

/// Format cookie: "<sid_b64>.<sig_b64>"
fn build_cookie_value(sid: &str, secret: &[u8]) -> String {
    let sid_b64 = b64(sid);
    let sig_b64 = sign_sid(sid, secret);
    format!("{sid_b64}.{sig_b64}")
}

/// Parse & verify cookie. Return Some(raw_sid) jika valid.
fn parse_and_verify_cookie(val: &str, secret: &[u8]) -> Option<String> {
    let (sid_b64, sig_b64) = val.split_once('.')?;
    let sid = b64_decode_to_string(sid_b64)?;
    // verifikasi HMAC
    let provided = Base64UrlUnpadded::decode_vec(sig_b64).ok()?;
    let mut mac = HmacSha256::new_from_slice(secret).ok()?;
    mac.update(sid.as_bytes());
    mac.verify_slice(&provided).ok()?;
    Some(sid)
}


/// Buat session baru + set cookie bertanda tangan.
/// TTL diambil dari cfg.session_token_ttl (detik).
pub async fn create_session(
    pool: &PgPool, 
    cfg: &Config, 
    user_id: &str, 
    is_admin: bool, 
    cookies: &Cookies) -> sqlx::Result<()>
 {
    let sid = Uuid::new_v4().to_string();
    let now = Utc::now();
    let ttl = ChronoDuration::seconds(cfg.session_token_ttl as i64).max(ChronoDuration::seconds(60));
    let exp = now + ttl;

    sqlx::query!(
        r#"
        INSERT INTO sessions (id, user_id, is_admin, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        sid,
        user_id,
        if is_admin { 1 } else { 0 },
        now.to_rfc3339(),
        exp.to_rfc3339()
    )
    .execute(pool)
    .await?;

    let mut c = Cookie::new(COOKIE_NAME, build_cookie_value(&sid, &cfg.hmac_secret));
    c.set_http_only(true);
    c.set_path("/");
    // Default aman untuk web apps
    c.set_same_site(tower_cookies::cookie::SameSite::Lax);
    // Jika ingin, aktifkan secure lewat reverse proxy/ENV (tidak dipaksa di sini)
    cookies.add(c);
    Ok(())
}

/// Hapus session (DB & cookie). Tidak error jika cookie tak ada.
pub async fn destroy_session(pool: &PgPool, cfg: &Config, cookies: &Cookies) -> sqlx::Result<()> {
    if let Some(c) = cookies.get(COOKIE_NAME) {
        let raw = c.value().to_string();
        if let Some(sid) = parse_and_verify_cookie(&raw, &cfg.hmac_secret) {
            let _ = sqlx::query!(r#"DELETE FROM sessions WHERE id = $1"#, sid)
                .execute(pool)
                .await;
        }
        cookies.remove(Cookie::from(COOKIE_NAME));
    }
    Ok(())
}

/// Returns Some((user_id, is_admin)) jika cookie valid & belum expired.
pub async fn current_user_id(pool: &PgPool, cfg: &Config, cookies: &Cookies) -> Option<(String, bool)> {
    let raw = cookies.get(COOKIE_NAME)?.value().to_string();
    let sid = parse_and_verify_cookie(&raw, &cfg.hmac_secret)?;

    let row = sqlx::query!(
        r#"SELECT user_id, is_admin, expires_at FROM sessions WHERE id = $1 LIMIT 1"#,
        sid
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    // expires_at disimpan sebagai TEXT (RFC3339)
    let exp = chrono::DateTime::parse_from_rfc3339(&row.expires_at)
        .ok()?
        .with_timezone(&Utc);

    if exp < Utc::now() {
        // cleanup jika expired
        let _ = sqlx::query!(r#"DELETE FROM sessions WHERE id = $1"#, sid)
            .execute(pool)
            .await;
        return None;
    }

    Some((row.user_id, row.is_admin != 0))
}
