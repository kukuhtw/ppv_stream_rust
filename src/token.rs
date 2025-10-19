// src/token.rs
use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{engine::general_purpose, Engine};

/// Buat token: "<session>.<exp>.<sig>"
pub fn sign_token(secret: &[u8], session: &str, exp: u64) -> String {
    let payload = format!("{}.{}", session, exp);
    let mut mac = <Hmac<Sha256>>::new_from_slice(secret)
        .expect("HMAC key must be valid");
    mac.update(payload.as_bytes());
    let sig = mac.finalize().into_bytes();
    let sig_b64 = general_purpose::URL_SAFE_NO_PAD.encode(sig);
    format!("{}.{}.{}", session, exp, sig_b64)
}
