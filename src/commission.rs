// src/commission.rs
//
// Standalone affiliate commission helper.
// Called after a successful video purchase (wallet, x402, or fiat) to:
//   1. Verify the video has an active affiliate program.
//   2. Resolve the affiliate user by username.
//   3. Deduct commission from the creator's balance and credit the affiliate.
//   4. Append wallet ledger rows and an affiliate_commissions audit row.
//
// This runs in its own DB transaction. If commission cannot be paid (creator
// balance is too low, affiliate username is invalid, etc.) the function returns
// Ok(0) so the calling purchase flow is never rolled back.

pub use sqlx::PgPool;
use sqlx::Row;

/// Attempt to pay an affiliate commission for a completed video purchase.
///
/// Returns `Ok(commission_cents)` when the commission was paid, `Ok(0)` when
/// skipped (affiliate disabled, missing, or invalid), and `Err` only for
/// unexpected database failures.
pub async fn process_affiliate_commission(
    pool: &PgPool,
    video_id: &str,
    buyer_id: &str,
    owner_id: &str,
    price_cents: i64,
    affiliate_username: &str,
    payment_method: &str,
    invoice_uid: Option<&str>,
) -> Result<i64, String> {
    let aff_username = affiliate_username.trim();
    if aff_username.is_empty() {
        return Ok(0);
    }

    // Look up affiliate settings for this video (runtime query — new table)
    let settings = sqlx::query(
        "SELECT commission_pct, is_enabled \
         FROM affiliate_settings WHERE video_id = $1 LIMIT 1",
    )
    .bind(video_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("affiliate settings: {e}"))?;

    let settings = match settings {
        Some(r) => r,
        None => return Ok(0), // no affiliate program configured
    };

    let is_enabled: bool = settings.try_get("is_enabled").unwrap_or(false);
    let commission_pct: i32 = settings.try_get("commission_pct").unwrap_or(0);

    if !is_enabled || commission_pct <= 0 {
        return Ok(0);
    }

    // Resolve affiliate user by username
    let aff_row = sqlx::query("SELECT id FROM users WHERE username = $1 LIMIT 1")
        .bind(aff_username)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("affiliate user lookup: {e}"))?;

    let aff_row = match aff_row {
        Some(r) => r,
        None => return Ok(0), // username not found — skip silently
    };

    let affiliate_id: String = aff_row.try_get("id").unwrap_or_default();
    if affiliate_id.is_empty() || affiliate_id == buyer_id || affiliate_id == owner_id {
        return Ok(0); // can't affiliate-pay yourself or the owner
    }

    // Commission is a percentage of the full video price
    let commission_cents = ((price_cents as i128) * (commission_pct as i128) / 100).max(0) as i64;
    if commission_cents == 0 {
        return Ok(0);
    }

    let mut tx = pool.begin().await.map_err(|e| format!("begin tx: {e}"))?;

    // Lock creator + affiliate rows in alphabetical order to prevent deadlocks
    let (id_a, id_b) = if owner_id < affiliate_id.as_str() {
        (owner_id.to_string(), affiliate_id.clone())
    } else {
        (affiliate_id.clone(), owner_id.to_string())
    };

    let _ = sqlx::query("SELECT id FROM users WHERE id IN ($1,$2) ORDER BY id FOR UPDATE")
        .bind(&id_a)
        .bind(&id_b)
        .fetch_all(&mut *tx)
        .await;

    // Read creator balance — if insufficient, skip without failing the purchase
    let creator_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(owner_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    if creator_bal < commission_cents {
        let _ = tx.rollback().await;
        return Ok(0);
    }

    let aff_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&affiliate_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    let creator_new = creator_bal - commission_cents;
    let aff_new = aff_bal + commission_cents;

    // Deduct from creator
    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(creator_new)
        .bind(owner_id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Err(format!("deduct creator: {e}"));
    }

    // Credit affiliate
    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(aff_new)
        .bind(&affiliate_id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Err(format!("credit affiliate: {e}"));
    }

    // Wallet ledger: creator side (transfer_out)
    if let Err(e) = sqlx::query(
        "INSERT INTO wallet_transactions \
         (user_id,txn_type,amount_cents,balance_after,status,ref_user_id,note) \
         VALUES ($1,'transfer_out',$2,$3,'completed',$4,$5)",
    )
    .bind(owner_id)
    .bind(commission_cents)
    .bind(creator_new)
    .bind(&affiliate_id)
    .bind(format!("Affiliate commission: {video_id}"))
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        return Err(format!("ledger creator: {e}"));
    }

    // Wallet ledger: affiliate side (transfer_in)
    if let Err(e) = sqlx::query(
        "INSERT INTO wallet_transactions \
         (user_id,txn_type,amount_cents,balance_after,status,ref_user_id,note) \
         VALUES ($1,'transfer_in',$2,$3,'completed',$4,$5)",
    )
    .bind(&affiliate_id)
    .bind(commission_cents)
    .bind(aff_new)
    .bind(owner_id)
    .bind(format!("Commission earned: {video_id}"))
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        return Err(format!("ledger affiliate: {e}"));
    }

    // Persist commission record
    if let Err(e) = sqlx::query(
        "INSERT INTO affiliate_commissions \
         (video_id,affiliate_id,buyer_id,owner_id,\
          purchase_price_cents,commission_cents,payment_method,ref_invoice_uid) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(video_id)
    .bind(&affiliate_id)
    .bind(buyer_id)
    .bind(owner_id)
    .bind(price_cents)
    .bind(commission_cents)
    .bind(payment_method)
    .bind(invoice_uid)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        return Err(format!("commissions row: {e}"));
    }

    tx.commit().await.map_err(|e| format!("commit: {e}"))?;
    Ok(commission_cents)
}
