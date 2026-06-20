use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::rngs::OsRng;
use rsa::{
    pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey},
    pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding},
    RsaPrivateKey, RsaPublicKey,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct ActorKeys {
    pub public_key_pem: String,
    pub private_key_pem: String,
}

/// Generate a fresh 2048-bit RSA key pair for an ActivityPub actor.
pub fn generate_actor_keys() -> anyhow::Result<ActorKeys> {
    let private_key =
        RsaPrivateKey::new(&mut OsRng, 2048).context("RSA 2048 key generation failed")?;
    let public_key = RsaPublicKey::from(&private_key);

    let private_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .context("private key PKCS#8 serialization failed")?;
    let public_pem = public_key
        .to_pkcs1_pem(LineEnding::LF)
        .context("public key PKCS#1 serialization failed")?;

    Ok(ActorKeys {
        public_key_pem: public_pem.to_string(),
        private_key_pem: private_pem.to_string(),
    })
}

/// Envelope-encrypt the private key PEM using HMAC-SHA256 as a stream-key derivation.
///
/// Format stored in DB: `v1:<base64(16-byte nonce)>:<base64(ciphertext)>`
pub fn encrypt_private_key(pem: &str, app_secret: &[u8]) -> anyhow::Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut root_mac =
        Hmac::<Sha256>::new_from_slice(app_secret).context("HMAC initialisation failed")?;
    root_mac.update(b"ppv-federation-actor-key-v1");
    let derived_key = root_mac.finalize().into_bytes();

    let mut nonce = [0u8; 16];
    rand::RngCore::fill_bytes(&mut OsRng, &mut nonce);

    let plaintext = pem.as_bytes();
    let mut ciphertext = plaintext.to_vec();

    // HMAC-SHA256 stream cipher: keystream[block] = HMAC(derived_key, nonce || block_idx)
    for (block_idx, chunk) in ciphertext.chunks_mut(32).enumerate() {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&derived_key).context("HMAC stream init failed")?;
        mac.update(&nonce);
        mac.update(&(block_idx as u64).to_le_bytes());
        let ks = mac.finalize().into_bytes();
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
    }

    Ok(format!("v1:{}:{}", B64.encode(nonce), B64.encode(&ciphertext)))
}

pub fn decrypt_private_key(encrypted: &str, app_secret: &[u8]) -> anyhow::Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let parts: Vec<&str> = encrypted.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != "v1" {
        anyhow::bail!("unsupported private key encryption format");
    }

    let nonce = B64.decode(parts[1]).context("nonce base64 decode failed")?;
    let mut ciphertext = B64.decode(parts[2]).context("ciphertext base64 decode failed")?;

    let mut root_mac =
        Hmac::<Sha256>::new_from_slice(app_secret).context("HMAC initialisation failed")?;
    root_mac.update(b"ppv-federation-actor-key-v1");
    let derived_key = root_mac.finalize().into_bytes();

    for (block_idx, chunk) in ciphertext.chunks_mut(32).enumerate() {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&derived_key).context("HMAC stream init failed")?;
        mac.update(&nonce);
        mac.update(&(block_idx as u64).to_le_bytes());
        let ks = mac.finalize().into_bytes();
        for (b, k) in chunk.iter_mut().zip(ks.iter()) {
            *b ^= k;
        }
    }

    String::from_utf8(ciphertext).context("decrypted key is not valid UTF-8")
}

/// Ensure a local federation actor record exists for `user_id`.
///
/// On first call, generates a key pair, encrypts the private key, and writes
/// the actor row. On subsequent calls, returns the existing public key PEM.
#[allow(dead_code)]
pub async fn ensure_local_actor_keys(
    pool: &PgPool,
    user_id: &str,
    username: &str,
    base_url: &str,
    domain: &str,
    app_secret: &[u8],
) -> anyhow::Result<String> {
    let actor_uri = format!("{}/users/{}", base_url, username);

    let existing: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT public_key_pem FROM federation_actors \
         WHERE local_user_id = $1 AND is_local = TRUE LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("actor record lookup failed")?;

    if let Some((Some(pem),)) = existing {
        return Ok(pem);
    }

    let keys = generate_actor_keys()?;
    let encrypted = encrypt_private_key(&keys.private_key_pem, app_secret)?;
    let public_key_id = format!("{}#main-key", actor_uri);

    sqlx::query(
        r#"
        INSERT INTO federation_actors (
            id, local_user_id, actor_uri, username, domain,
            inbox_url, outbox_url, followers_url, following_url,
            public_key_id, public_key_pem, private_key_encrypted, is_local
        ) VALUES (
            $1, $2, $3, $4, $5,
            $6, $7, $8, $9,
            $10, $11, $12, TRUE
        )
        ON CONFLICT (actor_uri) DO UPDATE SET
            public_key_id        = EXCLUDED.public_key_id,
            public_key_pem       = EXCLUDED.public_key_pem,
            private_key_encrypted = EXCLUDED.private_key_encrypted,
            updated_at           = NOW()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&actor_uri)
    .bind(username)
    .bind(domain)
    .bind(format!("{}/inbox", actor_uri))
    .bind(format!("{}/outbox", actor_uri))
    .bind(format!("{}/followers", actor_uri))
    .bind(format!("{}/following", actor_uri))
    .bind(&public_key_id)
    .bind(&keys.public_key_pem)
    .bind(&encrypted)
    .execute(pool)
    .await
    .context("actor key upsert failed")?;

    sqlx::query("UPDATE users SET actor_uri = $1 WHERE id = $2")
        .bind(&actor_uri)
        .bind(user_id)
        .execute(pool)
        .await
        .context("user actor_uri update failed")?;

    Ok(keys.public_key_pem)
}

/// Load the private key PEM and key-id for a local actor.
#[allow(dead_code)]
pub async fn load_actor_private_key(
    pool: &PgPool,
    user_id: &str,
    app_secret: &[u8],
) -> anyhow::Result<(String, String)> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT private_key_encrypted, public_key_id \
         FROM federation_actors WHERE local_user_id = $1 AND is_local = TRUE LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("actor key lookup failed")?;

    let (enc, key_id) = match row {
        Some((Some(e), Some(k))) => (e, k),
        _ => anyhow::bail!("no local actor key record for user {}", user_id),
    };

    let pem = decrypt_private_key(&enc, app_secret)?;
    Ok((pem, key_id))
}

/// Parse a PKCS#8 PEM private key string.
pub fn parse_private_key(pem: &str) -> anyhow::Result<RsaPrivateKey> {
    RsaPrivateKey::from_pkcs8_pem(pem).context("invalid PKCS#8 private key PEM")
}

/// Parse a PKCS#1 PEM public key string.
pub fn parse_public_key(pem: &str) -> anyhow::Result<RsaPublicKey> {
    RsaPublicKey::from_pkcs1_pem(pem).context("invalid PKCS#1 public key PEM")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_generation_round_trip() {
        let keys = generate_actor_keys().expect("key generation");
        assert!(keys.private_key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(keys.public_key_pem.contains("BEGIN RSA PUBLIC KEY"));

        // Round-trip parse
        parse_private_key(&keys.private_key_pem).expect("parse private key");
        parse_public_key(&keys.public_key_pem).expect("parse public key");
    }

    #[test]
    fn key_encrypt_decrypt_round_trip() {
        let secret = b"test-app-secret-32-bytes-exactly";
        let pem = "-----BEGIN PRIVATE KEY-----\ndummypemdata\n-----END PRIVATE KEY-----";
        let encrypted = encrypt_private_key(pem, secret).expect("encrypt");
        assert!(encrypted.starts_with("v1:"));
        let decrypted = decrypt_private_key(&encrypted, secret).expect("decrypt");
        assert_eq!(decrypted, pem);
    }

    #[test]
    fn wrong_secret_gives_wrong_plaintext() {
        let secret = b"correct-secret-xxxxxxxxxxxxxx00";
        let wrong = b"wrong-secret-xxxxxxxxxxxxxxxxx00";
        let pem = "some private key data";
        let encrypted = encrypt_private_key(pem, secret).expect("encrypt");
        let wrong_result = decrypt_private_key(&encrypted, wrong).unwrap_or_default();
        assert_ne!(wrong_result, pem);
    }
}
