use anyhow::Context;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::Utc;
use rsa::pkcs1v15::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use signature::{RandomizedSigner, SignatureEncoding, Verifier};
use std::collections::HashMap;

/// Maximum allowed age of a signed request (clock skew tolerance).
const MAX_AGE_SECS: i64 = 30;

// ── Digest header ──────────────────────────────────────────────────────────

/// Build a `Digest: SHA-256=<b64>` header value for a request body.
pub fn build_digest(body: &[u8]) -> String {
    let hash = Sha256::digest(body);
    format!("SHA-256={}", B64.encode(&hash))
}

/// Verify a `Digest` header against the actual request body.
pub fn verify_digest(body: &[u8], header: &str) -> anyhow::Result<()> {
    if header.trim() == build_digest(body) {
        Ok(())
    } else {
        anyhow::bail!("Digest header does not match request body")
    }
}

// ── Signing ────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub struct SignatureInput<'a> {
    pub key_id: &'a str,
    /// PKCS#8 PEM private key for the local actor.
    pub private_key_pem: &'a str,
    pub method: &'a str,
    pub path_and_query: &'a str,
    pub host: &'a str,
    /// RFC 2822 date string (e.g. `Tue, 20 Jun 2026 12:00:00 GMT`).
    pub date: &'a str,
    /// Set when signing a request that has a body.
    pub digest: Option<&'a str>,
}

/// Create an HTTP Signature for an outbound ActivityPub request.
///
/// Returns the value for the `Signature:` header.
#[allow(dead_code)]
pub fn create_signature(input: &SignatureInput<'_>) -> anyhow::Result<String> {
    let private_key = crate::federation::keys::parse_private_key(input.private_key_pem)?;

    let mut headers: HashMap<&str, &str> = HashMap::new();
    headers.insert("host", input.host);
    headers.insert("date", input.date);

    let mut names: Vec<&str> = vec!["(request-target)", "host", "date"];
    if let Some(d) = input.digest {
        headers.insert("digest", d);
        names.push("digest");
    }

    let signing_string = build_signing_string(input.method, input.path_and_query, &names, &headers);

    let signing_key = SigningKey::<Sha256>::new(private_key);
    let sig = signing_key.sign_with_rng(&mut rand::rngs::OsRng, signing_string.as_bytes());
    let sig_b64 = B64.encode(sig.to_bytes());

    Ok(format!(
        r#"keyId="{}",algorithm="rsa-sha256",headers="{}",signature="{}""#,
        input.key_id,
        names.join(" "),
        sig_b64
    ))
}

// ── Verification ───────────────────────────────────────────────────────────

pub struct IncomingSignature<'a> {
    pub method: &'a str,
    pub path_and_query: &'a str,
    /// Raw value of the `Signature:` header.
    pub signature_header: &'a str,
    /// All request headers (lower-cased names).
    pub request_headers: &'a HashMap<String, String>,
    /// PKCS#1 PEM public key fetched for the signing actor.
    pub public_key_pem: &'a str,
}

/// Verify an incoming HTTP Signature.
///
/// Returns the `keyId` from the signature on success, so the caller can
/// confirm it matches the actor they fetched the key for.
pub fn verify_signature(input: &IncomingSignature<'_>) -> anyhow::Result<String> {
    let params =
        parse_signature_header(input.signature_header).context("malformed Signature header")?;

    let key_id = params
        .get("keyId")
        .ok_or_else(|| anyhow::anyhow!("Signature header missing keyId"))?
        .clone();
    let header_names_str = params.get("headers").map(|s| s.as_str()).unwrap_or("date");
    let sig_b64 = params
        .get("signature")
        .ok_or_else(|| anyhow::anyhow!("Signature header missing signature value"))?;

    // Enforce max request age
    if let Some(date_val) = input.request_headers.get("date") {
        enforce_max_age(date_val).context("request age check failed")?;
    }

    let names: Vec<&str> = header_names_str.split_whitespace().collect();
    let header_map: HashMap<&str, &str> = input
        .request_headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let signing_string =
        build_signing_string(input.method, input.path_and_query, &names, &header_map);

    let sig_bytes = B64
        .decode(sig_b64)
        .context("Signature base64 decode failed")?;

    let public_key = crate::federation::keys::parse_public_key(input.public_key_pem)?;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let sig = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("invalid signature bytes: {e}"))?;

    verifying_key
        .verify(signing_string.as_bytes(), &sig)
        .map_err(|_| anyhow::anyhow!("HTTP Signature RSA-SHA256 verification failed"))?;

    Ok(key_id)
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn build_signing_string(
    method: &str,
    path_and_query: &str,
    header_names: &[&str],
    headers: &HashMap<&str, &str>,
) -> String {
    header_names
        .iter()
        .map(|&name| {
            if name == "(request-target)" {
                format!(
                    "(request-target): {} {}",
                    method.to_lowercase(),
                    path_and_query
                )
            } else {
                let val = headers.get(name).copied().unwrap_or("");
                format!("{}: {}", name, val)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_signature_header(header: &str) -> anyhow::Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for segment in header.split(',') {
        let segment = segment.trim();
        if let Some((k, v)) = segment.split_once('=') {
            let key = k.trim().to_string();
            let value = v.trim().trim_matches('"').to_string();
            map.insert(key, value);
        }
    }
    if map.is_empty() {
        anyhow::bail!("Signature header contained no recognisable parameters");
    }
    Ok(map)
}

fn enforce_max_age(date_str: &str) -> anyhow::Result<()> {
    let parsed = chrono::DateTime::parse_from_rfc2822(date_str)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(date_str))
        .context("Date header is not a valid RFC 2822 / RFC 3339 timestamp")?;
    let age = (Utc::now() - parsed.with_timezone(&Utc))
        .num_seconds()
        .abs();
    if age > MAX_AGE_SECS {
        anyhow::bail!(
            "request is {} seconds old; maximum allowed is {}",
            age,
            MAX_AGE_SECS
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_mismatch_is_rejected() {
        let body = b"hello activitypub";
        let digest = build_digest(body);
        assert!(verify_digest(body, &digest).is_ok());
        assert!(verify_digest(b"tampered body", &digest).is_err());
    }

    #[test]
    fn digest_format_is_correct() {
        let digest = build_digest(b"");
        // SHA-256 of empty string is well-known
        assert_eq!(
            digest,
            "SHA-256=47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
    }

    #[test]
    fn signature_round_trip() {
        let keys = crate::federation::keys::generate_actor_keys().expect("keygen");

        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();

        let input = SignatureInput {
            key_id: "https://example.com/users/alice#main-key",
            private_key_pem: &keys.private_key_pem,
            method: "POST",
            path_and_query: "/users/bob/inbox",
            host: "example.com",
            date: &date,
            digest: Some("SHA-256=47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="),
        };

        let sig_header = create_signature(&input).expect("sign");
        assert!(sig_header.contains("rsa-sha256"));
        assert!(sig_header.contains("keyId="));

        let mut req_headers: HashMap<String, String> = HashMap::new();
        req_headers.insert("host".into(), "example.com".into());
        req_headers.insert("date".into(), date.clone());
        req_headers.insert(
            "digest".into(),
            "SHA-256=47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=".into(),
        );

        let verified = verify_signature(&IncomingSignature {
            method: "POST",
            path_and_query: "/users/bob/inbox",
            signature_header: &sig_header,
            request_headers: &req_headers,
            public_key_pem: &keys.public_key_pem,
        });

        assert!(verified.is_ok(), "verify failed: {:?}", verified.err());
        assert_eq!(
            verified.unwrap(),
            "https://example.com/users/alice#main-key"
        );
    }
}
