//! Cross-instance revenue sharing for index-only federation.
//!
//! ## Flow
//!
//! 1. When instance A displays a remote video from instance B in its catalog,
//!    it may append a signed referral token to the checkout link:
//!    `https://b.example/checkout/vid?fed_ref=<token>`
//!
//! 2. Instance B records the referral token, verifies the RSA signature, and
//!    associates it with the viewer session.
//!
//! 3. When the viewer completes a payment, instance B calls
//!    `process_revenue_share` which idempotently calculates the traffic
//!    provider's share (in basis points from `revenue_share_policies`) and
//!    appends a credit ledger entry.
//!
//! 4. Admins can query the provider and affiliate settlement reports to see
//!    amounts owed.

use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── Referral payload ───────────────────────────────────────────────────────

/// The cleartext part of a signed referral token.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReferralClaims {
    /// Domain of the referring instance (bare hostname).
    pub domain: String,
    /// Unix timestamp when the token was created.
    pub ts: i64,
    /// Random nonce to prevent replay.
    pub nonce: String,
}

/// Build a signed referral token for `domain`.
///
/// Format: `<base64url(json_claims)>.<base64url(rsa_sha256_sig)>`
///
/// The signature covers exactly the base64url-encoded claims string.
/// The caller supplies the referring actor's RSA private key in PKCS#8 PEM.
#[allow(dead_code)]
pub fn build_referral_payload(domain: &str, private_key_pem: &str) -> anyhow::Result<String> {
    use signature::{RandomizedSigner, SignatureEncoding};

    let mut nonce_bytes = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
    let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);

    let claims = ReferralClaims {
        domain: domain.to_string(),
        ts: chrono::Utc::now().timestamp(),
        nonce,
    };

    let claims_json = serde_json::to_string(&claims).context("claims serialisation failed")?;
    let claims_b64 = URL_SAFE_NO_PAD.encode(claims_json.as_bytes());

    let private_key = crate::federation::keys::parse_private_key(private_key_pem)
        .context("private key parse failed")?;

    use rsa::pkcs1v15::SigningKey;
    use sha2::Sha256;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let sig = signing_key
        .sign_with_rng(&mut rand::rngs::OsRng, claims_b64.as_bytes())
        .to_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(&sig);

    Ok(format!("{}.{}", claims_b64, sig_b64))
}

/// Verify a signed referral token and return the referring domain.
///
/// Also enforces a maximum age of 24 hours to limit replay window.
pub fn verify_referral_payload(
    token: &str,
    public_key_pem: &str,
) -> anyhow::Result<ReferralClaims> {
    use signature::Verifier;

    let (claims_b64, sig_b64) = token
        .rsplit_once('.')
        .ok_or_else(|| anyhow::anyhow!("malformed referral token"))?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .context("base64 decode of signature failed")?;

    let public_key = crate::federation::keys::parse_public_key(public_key_pem)
        .context("public key parse failed")?;

    use rsa::pkcs1v15::VerifyingKey;
    use sha2::Sha256;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    use rsa::pkcs1v15::Signature;
    let sig = Signature::try_from(sig_bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("invalid signature bytes"))?;

    verifying_key
        .verify(claims_b64.as_bytes(), &sig)
        .map_err(|_| anyhow::anyhow!("referral signature verification failed"))?;

    let claims_json = URL_SAFE_NO_PAD
        .decode(claims_b64)
        .context("base64 decode of claims failed")?;
    let claims: ReferralClaims =
        serde_json::from_slice(&claims_json).context("claims JSON parse failed")?;

    // Reject tokens older than 24 hours
    let age_secs = chrono::Utc::now().timestamp() - claims.ts;
    if !(-300..=86_400).contains(&age_secs) {
        anyhow::bail!(
            "referral token is expired or from the future (age {}s)",
            age_secs
        );
    }

    Ok(claims)
}

// ── Referral recording ─────────────────────────────────────────────────────

/// Record a presented referral token in `federation_referrals`.
///
/// `verified` should be `true` only when `verify_referral_payload` succeeded.
/// Returns the new row's primary key.
#[allow(dead_code)]
pub async fn record_referral(
    pool: &PgPool,
    referring_domain: &str,
    raw_payload: &str,
    viewer_nonce: Option<&str>,
    verified: bool,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO federation_referrals
            (id, referring_domain, raw_payload, viewer_nonce, verified)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(referring_domain)
    .bind(raw_payload)
    .bind(viewer_nonce)
    .bind(verified)
    .execute(pool)
    .await
    .context("federation_referrals insert failed")?;
    Ok(id)
}

// ── Revenue calculation ────────────────────────────────────────────────────

/// Calculate the referring instance's share using integer arithmetic.
///
/// `basis_points`: 100 = 1 %, 500 = 5 %, 10 000 = 100 %.
/// Uses floor division; the remainder stays with the destination instance.
#[allow(dead_code)]
pub fn calculate_share_cents(gross_cents: i64, basis_points: i32) -> i64 {
    if basis_points <= 0 || gross_cents <= 0 {
        return 0;
    }
    gross_cents * basis_points as i64 / 10_000
}

/// Validate that `basis_points` is in the allowed range \[0, 5 000\].
pub fn validate_basis_points(bp: i32) -> anyhow::Result<()> {
    if !(0..=5_000).contains(&bp) {
        anyhow::bail!("basis_points {} is out of range [0, 5000] (max 50 %)", bp);
    }
    Ok(())
}

// ── Idempotent revenue processing ─────────────────────────────────────────

/// Record a revenue share entry for a completed payment.
///
/// If a row already exists for `(invoice_id, invoice_type)` this function
/// returns `Ok(None)` without writing anything — idempotent by design.
///
/// Returns `Ok(Some(share_cents))` when a new share entry was created, or
/// `Ok(None)` when skipped (no policy, no referral, or already processed).
#[allow(dead_code)]
pub async fn process_revenue_share(
    pool: &PgPool,
    invoice_id: &str,
    invoice_type: &str,
    referral_id: Option<Uuid>,
    referring_domain: Option<&str>,
    gross_cents: i64,
) -> anyhow::Result<Option<i64>> {
    // Idempotency check
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT share_cents FROM federation_revenue_shares \
         WHERE invoice_id = $1 AND invoice_type = $2 LIMIT 1",
    )
    .bind(invoice_id)
    .bind(invoice_type)
    .fetch_optional(pool)
    .await
    .context("revenue share idempotency check failed")?;

    if existing.is_some() {
        return Ok(None);
    }

    // Look up the share policy for this domain
    let domain = match referring_domain {
        Some(d) if !d.is_empty() => d,
        _ => return Ok(None),
    };

    let policy: Option<(i32, bool)> = sqlx::query_as(
        "SELECT share_basis_points, is_active \
         FROM revenue_share_policies WHERE instance_domain = $1 LIMIT 1",
    )
    .bind(domain)
    .fetch_optional(pool)
    .await
    .context("revenue share policy lookup failed")?;

    let Some((basis_points, is_active)) = policy else {
        return Ok(None); // no policy configured for this domain
    };

    if !is_active {
        return Ok(None);
    }

    validate_basis_points(basis_points)?;
    let share_cents = calculate_share_cents(gross_cents, basis_points);

    if share_cents == 0 {
        return Ok(None);
    }

    let share_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO federation_revenue_shares
            (id, invoice_id, invoice_type, referral_id, referring_domain,
             gross_cents, share_basis_points, share_cents, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')
        ON CONFLICT (invoice_id, invoice_type) DO NOTHING
        "#,
    )
    .bind(share_id)
    .bind(invoice_id)
    .bind(invoice_type)
    .bind(referral_id)
    .bind(domain)
    .bind(gross_cents)
    .bind(basis_points)
    .bind(share_cents)
    .execute(pool)
    .await
    .context("federation_revenue_shares insert failed")?;

    // Append the initial credit ledger entry
    record_ledger_entry(
        pool,
        share_id,
        "credit",
        share_cents,
        Some(&format!("traffic referral from {}", domain)),
    )
    .await?;

    Ok(Some(share_cents))
}

/// Append an immutable ledger line for a revenue share record.
#[allow(dead_code)]
pub async fn record_ledger_entry(
    pool: &PgPool,
    revenue_share_id: Uuid,
    entry_type: &str,
    amount_cents: i64,
    description: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO revenue_ledger_entries
            (id, revenue_share_id, entry_type, amount_cents, description)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(revenue_share_id)
    .bind(entry_type)
    .bind(amount_cents)
    .bind(description)
    .execute(pool)
    .await
    .context("revenue_ledger_entries insert failed")?;
    Ok(())
}

/// Record a reversal (refund or chargeback) for an existing revenue share.
///
/// Sets `status = 'reversed'` and appends a debit/chargeback ledger line.
/// `entry_type` must be `"refund"` or `"chargeback"`.
#[allow(dead_code)]
pub async fn reverse_revenue_share(
    pool: &PgPool,
    invoice_id: &str,
    invoice_type: &str,
    entry_type: &str,
) -> anyhow::Result<()> {
    if entry_type != "refund" && entry_type != "chargeback" {
        anyhow::bail!(
            "entry_type must be 'refund' or 'chargeback', got '{}'",
            entry_type
        );
    }

    let row: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT id, share_cents FROM federation_revenue_shares \
         WHERE invoice_id = $1 AND invoice_type = $2 AND status = 'pending' LIMIT 1",
    )
    .bind(invoice_id)
    .bind(invoice_type)
    .fetch_optional(pool)
    .await
    .context("revenue share reversal lookup failed")?;

    let Some((share_id, share_cents)) = row else {
        return Ok(()); // nothing to reverse
    };

    sqlx::query(
        "UPDATE federation_revenue_shares \
         SET status = 'reversed', updated_at = NOW() WHERE id = $1",
    )
    .bind(share_id)
    .execute(pool)
    .await
    .context("revenue share status update failed")?;

    record_ledger_entry(pool, share_id, entry_type, share_cents, None).await
}

// ── Settlement reporting ───────────────────────────────────────────────────

/// Summary row returned by the provider settlement report.
#[derive(Debug, serde::Serialize)]
pub struct ProviderSettlementRow {
    pub referring_domain: String,
    pub pending_cents: i64,
    pub settled_cents: i64,
    pub reversed_cents: i64,
    pub payment_count: i64,
}

/// Aggregate revenue owed to each referring provider instance.
pub async fn provider_settlement_report(
    pool: &PgPool,
) -> anyhow::Result<Vec<ProviderSettlementRow>> {
    let rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            referring_domain,
            COALESCE(SUM(CASE WHEN status = 'pending'  THEN share_cents ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN status = 'settled'  THEN share_cents ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN status = 'reversed' THEN share_cents ELSE 0 END), 0),
            COUNT(*)
        FROM federation_revenue_shares
        WHERE referring_domain IS NOT NULL
        GROUP BY referring_domain
        ORDER BY pending_cents DESC, referring_domain
        "#,
    )
    .fetch_all(pool)
    .await
    .context("provider settlement report query failed")?;

    Ok(rows
        .into_iter()
        .map(
            |(domain, pending, settled, reversed, count)| ProviderSettlementRow {
                referring_domain: domain,
                pending_cents: pending,
                settled_cents: settled,
                reversed_cents: reversed,
                payment_count: count,
            },
        )
        .collect())
}

/// Summary row returned by the affiliate settlement report.
///
/// "Affiliate" here refers to domains for which we hold revenue shares that
/// still need to be paid out.  The report groups ledger entries by type so
/// the administrator can see the net position for each domain.
#[derive(Debug, serde::Serialize)]
pub struct AffiliateSettlementRow {
    pub referring_domain: String,
    pub share_basis_points: i32,
    pub total_gross_cents: i64,
    pub total_share_cents: i64,
    pub pending_count: i64,
    pub settled_count: i64,
    pub reversed_count: i64,
}

/// Per-domain settlement summary — grouped by referring instance.
pub async fn affiliate_settlement_report(
    pool: &PgPool,
) -> anyhow::Result<Vec<AffiliateSettlementRow>> {
    let rows: Vec<(String, i32, i64, i64, i64, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            rs.referring_domain,
            COALESCE(rsp.share_basis_points, 0),
            COALESCE(SUM(rs.gross_cents), 0)                                      AS total_gross,
            COALESCE(SUM(rs.share_cents), 0)                                      AS total_share,
            COUNT(*) FILTER (WHERE rs.status = 'pending')                         AS pending,
            COUNT(*) FILTER (WHERE rs.status = 'settled')                         AS settled,
            COUNT(*) FILTER (WHERE rs.status = 'reversed')                        AS reversed
        FROM federation_revenue_shares rs
        LEFT JOIN revenue_share_policies rsp ON rsp.instance_domain = rs.referring_domain
        WHERE rs.referring_domain IS NOT NULL
        GROUP BY rs.referring_domain, rsp.share_basis_points
        ORDER BY total_share DESC, rs.referring_domain
        "#,
    )
    .fetch_all(pool)
    .await
    .context("affiliate settlement report query failed")?;

    Ok(rows
        .into_iter()
        .map(
            |(domain, bp, gross, share, pending, settled, reversed)| AffiliateSettlementRow {
                referring_domain: domain,
                share_basis_points: bp,
                total_gross_cents: gross,
                total_share_cents: share,
                pending_count: pending,
                settled_count: settled,
                reversed_count: reversed,
            },
        )
        .collect())
}

// ── Policy management ──────────────────────────────────────────────────────

/// Upsert a revenue share policy for a remote instance.
pub async fn set_share_policy(
    pool: &PgPool,
    instance_domain: &str,
    basis_points: i32,
    created_by: &str,
) -> anyhow::Result<()> {
    validate_basis_points(basis_points)?;
    sqlx::query(
        r#"
        INSERT INTO revenue_share_policies
            (id, instance_domain, share_basis_points, is_active, created_by)
        VALUES ($1, $2, $3, TRUE, $4)
        ON CONFLICT (instance_domain) DO UPDATE
            SET share_basis_points = EXCLUDED.share_basis_points,
                is_active          = TRUE,
                updated_at         = NOW()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(instance_domain)
    .bind(basis_points)
    .bind(created_by)
    .execute(pool)
    .await
    .context("revenue_share_policies upsert failed")?;
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculate_share_zero_when_no_bp() {
        assert_eq!(calculate_share_cents(10_000, 0), 0);
    }

    #[test]
    fn calculate_share_five_percent() {
        // 5% of $100.00 = $5.00
        assert_eq!(calculate_share_cents(10_000, 500), 500);
    }

    #[test]
    fn calculate_share_floor_division() {
        // 5% of $1.01 = 0.0505 → floor → 5 cents
        assert_eq!(calculate_share_cents(101, 500), 5);
    }

    #[test]
    fn calculate_share_negative_gross() {
        assert_eq!(calculate_share_cents(-100, 500), 0);
    }

    #[test]
    fn validate_basis_points_accepts_range() {
        assert!(validate_basis_points(0).is_ok());
        assert!(validate_basis_points(500).is_ok());
        assert!(validate_basis_points(5000).is_ok());
    }

    #[test]
    fn validate_basis_points_rejects_out_of_range() {
        assert!(validate_basis_points(-1).is_err());
        assert!(validate_basis_points(5001).is_err());
    }

    #[test]
    fn referral_payload_round_trip() {
        // Generate a fresh key pair and verify the payload can be decoded.
        let keys = crate::federation::keys::generate_actor_keys().unwrap();
        let token = build_referral_payload("remote.example", &keys.private_key_pem).unwrap();
        let claims = verify_referral_payload(&token, &keys.public_key_pem).unwrap();
        assert_eq!(claims.domain, "remote.example");
        assert!(!claims.nonce.is_empty());
    }

    #[test]
    fn referral_payload_wrong_key_fails() {
        let keys_a = crate::federation::keys::generate_actor_keys().unwrap();
        let keys_b = crate::federation::keys::generate_actor_keys().unwrap();
        let token = build_referral_payload("remote.example", &keys_a.private_key_pem).unwrap();
        // Verifying with the wrong public key must fail.
        let result = verify_referral_payload(&token, &keys_b.public_key_pem);
        assert!(result.is_err(), "wrong key should not verify");
    }

    #[test]
    fn malformed_token_is_rejected() {
        let keys = crate::federation::keys::generate_actor_keys().unwrap();
        let result = verify_referral_payload("notavalidtoken", &keys.public_key_pem);
        assert!(result.is_err());
    }
}
