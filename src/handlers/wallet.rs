// src/handlers/wallet.rs
// Mini wallet — pure database ledger, no blockchain.
//
// Also houses the wallet-pay-video endpoint: buy a video with internal balance.
// Split: creator receives `creator_split_bp/10000` of price; platform retains the rest.
//
// Balance lives in `users.balance_cents` (integer USD cents).
// Every mutation appends a row to `wallet_transactions` for a complete audit trail.
//
// Flows
//   deposit    → user submits request (pending) → admin approves → balance credited
//   withdrawal → balance held immediately (pending) → admin marks paid / rejects+refunds
//   transfer   → instant, atomic, no admin needed; both sides get a ledger row

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tower_cookies::Cookies;

use crate::commission;
use crate::config::Config;
use crate::payment_settings::load_payment_settings;
use crate::sessions;

pub const MIN_DEPOSIT_CENTS: i64 = 1_000; // $10
pub const MIN_WITHDRAWAL_CENTS: i64 = 5_000; // $50
pub const MIN_TRANSFER_CENTS: i64 = 100; // $1

#[derive(Clone)]
pub struct WalletState {
    pub pool: PgPool,
    pub cfg: Config,
}

fn cents_to_display(c: i64) -> String {
    format!("${}.{:02}", c / 100, (c % 100).unsigned_abs())
}

// ─── GET /api/wallet/balance ─────────────────────────────────────────────────

pub async fn wallet_balance(State(st): State<WalletState>, cookies: Cookies) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let row = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&uid)
        .fetch_optional(&st.pool)
        .await;

    let bal: i64 = match row {
        Ok(Some(r)) => r.try_get("balance_cents").unwrap_or(0),
        _ => 0,
    };

    Json(json!({
        "ok": true,
        "balance_cents":   bal,
        "balance_display": cents_to_display(bal)
    }))
}

// ─── GET /api/wallet/transactions ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TxnQuery {
    pub limit: Option<i64>,
}

#[derive(Serialize)]
struct TxnRow {
    id: i64,
    txn_type: String,
    amount_cents: i64,
    balance_after: i64,
    status: String,
    ref_username: Option<String>,
    note: Option<String>,
    admin_note: Option<String>,
    created_at: String,
    transfer_ref: Option<String>,
    has_proof: bool,
}

pub async fn wallet_transactions(
    State(st): State<WalletState>,
    cookies: Cookies,
    Query(q): Query<TxnQuery>,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let limit = q.limit.unwrap_or(50).min(200);

    let rows = sqlx::query(
        r#"SELECT wt.id, wt.txn_type, wt.amount_cents, wt.balance_after,
                  wt.status, wt.note, wt.admin_note,
                  wt.created_at::TEXT AS created_at,
                  wt.transfer_ref,
                  (wt.proof_path IS NOT NULL) AS has_proof,
                  u2.username AS ref_username
           FROM wallet_transactions wt
           LEFT JOIN users u2 ON u2.id = wt.ref_user_id
           WHERE wt.user_id = $1
           ORDER BY wt.created_at DESC
           LIMIT $2"#,
    )
    .bind(&uid)
    .bind(limit)
    .fetch_all(&st.pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let items: Vec<TxnRow> = rows
        .iter()
        .map(|r| TxnRow {
            id: r.try_get("id").unwrap_or(0),
            txn_type: r.try_get("txn_type").unwrap_or_default(),
            amount_cents: r.try_get("amount_cents").unwrap_or(0),
            balance_after: r.try_get("balance_after").unwrap_or(0),
            status: r.try_get("status").unwrap_or_default(),
            ref_username: r.try_get("ref_username").unwrap_or(None),
            note: r.try_get("note").unwrap_or(None),
            admin_note: r.try_get("admin_note").unwrap_or(None),
            created_at: r
                .try_get::<Option<String>, _>("created_at")
                .unwrap_or(None)
                .unwrap_or_default(),
            transfer_ref: r.try_get("transfer_ref").unwrap_or(None),
            has_proof: r
                .try_get::<Option<bool>, _>("has_proof")
                .unwrap_or(None)
                .unwrap_or(false),
        })
        .collect();

    Json(json!({"ok": true, "items": items}))
}

// ─── POST /api/wallet/deposit ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DepositPayload {
    pub amount_cents: i64,
    pub note: Option<String>,
    /// Bank / e-wallet transfer reference shown on the sender's receipt.
    #[serde(default)]
    pub transfer_ref: Option<String>,
    /// Account holder name used for the transfer.
    #[serde(default)]
    pub sender_name: Option<String>,
}

pub async fn wallet_deposit(
    State(st): State<WalletState>,
    cookies: Cookies,
    Json(p): Json<DepositPayload>,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    if p.amount_cents < MIN_DEPOSIT_CENTS {
        return Json(json!({
            "ok": false,
            "error": format!("minimum deposit is {}", cents_to_display(MIN_DEPOSIT_CENTS))
        }));
    }

    let cur_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&uid)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    let res = sqlx::query(
        r#"INSERT INTO wallet_transactions
               (user_id, txn_type, amount_cents, balance_after, status, note)
           VALUES ($1, 'deposit', $2, $3, 'pending', $4)
           RETURNING id"#,
    )
    .bind(&uid)
    .bind(p.amount_cents)
    .bind(cur_bal)
    .bind(p.note.as_deref())
    .fetch_one(&st.pool)
    .await;

    match res {
        Ok(r) => {
            let id: i64 = r.try_get("id").unwrap_or(0);
            let transfer_ref = p
                .transfer_ref
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(120).collect::<String>());
            let sender_name = p
                .sender_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.chars().take(120).collect::<String>());
            if transfer_ref.is_some() || sender_name.is_some() {
                let _ = sqlx::query(
                    "UPDATE wallet_transactions SET transfer_ref = $2, sender_name = $3 WHERE id = $1",
                )
                .bind(id)
                .bind(transfer_ref)
                .bind(sender_name)
                .execute(&st.pool)
                .await;
            }
            Json(json!({
                "ok": true,
                "txn_id": id,
                "message": "Deposit request submitted. Upload your transfer proof, then wait for admin approval."
            }))
        }
        Err(e) => {
            tracing::error!("wallet deposit insert failed: {e}");
            Json(json!({"ok": false, "error": "could not create deposit request"}))
        }
    }
}

// ─── GET /api/wallet/deposit_info ────────────────────────────────────────────
// Where to send money for a deposit. Configured by the platform through
// WALLET_DEPOSIT_INSTRUCTIONS (multi-line text: bank, account number, holder,
// e-wallet, etc.). `\n` in the env value is turned into a line break.

pub async fn wallet_deposit_info(State(st): State<WalletState>) -> impl IntoResponse {
    let instructions = std::env::var("WALLET_DEPOSIT_INSTRUCTIONS")
        .unwrap_or_default()
        .replace("\\n", "\n");
    Json(json!({
        "ok": true,
        "configured": !instructions.trim().is_empty(),
        "instructions": instructions,
        "min_deposit_cents": MIN_DEPOSIT_CENTS,
        "min_deposit_display": cents_to_display(MIN_DEPOSIT_CENTS),
        "usd_to_idr": crate::fx::usd_to_idr(st.cfg.dollar_usd_to_rupiah),
        "proof_max_bytes": MAX_PROOF_BYTES,
        "proof_types": ["image/jpeg", "image/png", "image/webp", "application/pdf"],
    }))
}

// ─── POST /api/wallet/deposit_proof (multipart: txn_id, file) ────────────────
// Attach a transfer proof to the caller's own pending deposit.

pub const MAX_PROOF_BYTES: usize = 5 * 1024 * 1024;

pub async fn wallet_deposit_proof(
    State(st): State<WalletState>,
    cookies: Cookies,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let mut txn_id: Option<i64> = None;
    let mut file_bytes: Option<Vec<u8>> = None;

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => return Json(json!({"ok": false, "error": "invalid upload"})),
        };
        match field.name().unwrap_or("") {
            "txn_id" => {
                let text = field.text().await.unwrap_or_default();
                txn_id = text.trim().parse().ok();
            }
            "file" => match field.bytes().await {
                Ok(b) if b.len() <= MAX_PROOF_BYTES => file_bytes = Some(b.to_vec()),
                Ok(_) => return Json(json!({"ok": false, "error": "file is larger than 5 MB"})),
                Err(_) => return Json(json!({"ok": false, "error": "file is larger than 5 MB"})),
            },
            _ => {}
        }
    }

    let Some(txn_id) = txn_id else {
        return Json(json!({"ok": false, "error": "txn_id required"}));
    };
    let Some(bytes) = file_bytes.filter(|b| !b.is_empty()) else {
        return Json(json!({"ok": false, "error": "file required"}));
    };

    // Validate by content, not by file name.
    let (mime, ext) = match infer::get(&bytes).map(|k| k.mime_type()) {
        Some("image/jpeg") => ("image/jpeg", "jpg"),
        Some("image/png") => ("image/png", "png"),
        Some("image/webp") => ("image/webp", "webp"),
        Some("application/pdf") => ("application/pdf", "pdf"),
        _ => return Json(json!({"ok": false, "error": "only JPG, PNG, WEBP or PDF are accepted"})),
    };

    // Only the owner of a still-pending deposit may attach a proof.
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM wallet_transactions
                       WHERE id = $1 AND user_id = $2
                         AND txn_type = 'deposit' AND status = 'pending')",
    )
    .bind(txn_id)
    .bind(&uid)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(false);
    if !owned {
        return Json(json!({"ok": false, "error": "pending deposit not found"}));
    }

    let dir = std::path::Path::new(&st.cfg.upload_dir).join("deposit_proofs");
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::error!("deposit proof mkdir failed: {e}");
        return Json(json!({"ok": false, "error": "could not store file"}));
    }
    let file_name = format!("{}-{}.{}", txn_id, uuid::Uuid::new_v4(), ext);
    let path = dir.join(&file_name);
    if let Err(e) = tokio::fs::write(&path, &bytes).await {
        tracing::error!("deposit proof write failed: {e}");
        return Json(json!({"ok": false, "error": "could not store file"}));
    }

    let res = sqlx::query(
        "UPDATE wallet_transactions
            SET proof_path = $2, proof_mime = $3, proof_uploaded_at = NOW(), updated_at = NOW()
          WHERE id = $1",
    )
    .bind(txn_id)
    .bind(path.to_string_lossy().to_string())
    .bind(mime)
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => Json(json!({"ok": true, "txn_id": txn_id, "message": "Transfer proof uploaded."})),
        Err(e) => {
            tracing::error!("deposit proof update failed: {e}");
            let _ = tokio::fs::remove_file(&path).await;
            Json(json!({"ok": false, "error": "could not save proof"}))
        }
    }
}

// ─── GET /admin/wallet/transactions/:id/proof ────────────────────────────────
// Admin-only: stream the uploaded proof and its transfer details.

pub async fn admin_wallet_deposit_proof(
    State(st): State<WalletState>,
    cookies: Cookies,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};

    match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some((_, true)) => {}
        Some(_) => return (StatusCode::FORBIDDEN, "admin only").into_response(),
        None => return (StatusCode::UNAUTHORIZED, "not logged in").into_response(),
    }

    let row = sqlx::query(
        "SELECT proof_path, proof_mime FROM wallet_transactions WHERE id = $1 AND txn_type = 'deposit'",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await
    .ok()
    .flatten();

    let Some(row) = row else {
        return (StatusCode::NOT_FOUND, "deposit not found").into_response();
    };
    let path: Option<String> = row.try_get("proof_path").ok().flatten();
    let mime: String = row
        .try_get::<Option<String>, _>("proof_mime")
        .ok()
        .flatten()
        .unwrap_or_else(|| "application/octet-stream".into());
    let Some(path) = path else {
        return (StatusCode::NOT_FOUND, "no proof uploaded").into_response();
    };

    // The stored path must stay inside the proofs directory.
    let base = std::path::Path::new(&st.cfg.upload_dir).join("deposit_proofs");
    let (Ok(base), Ok(full)) = (std::fs::canonicalize(&base), std::fs::canonicalize(&path)) else {
        return (StatusCode::NOT_FOUND, "proof file missing").into_response();
    };
    if !full.starts_with(&base) {
        return (StatusCode::FORBIDDEN, "invalid proof path").into_response();
    }

    match tokio::fs::read(&full).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-store".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("inline; filename=\"deposit-{id}-proof\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "proof file missing").into_response(),
    }
}

// ─── POST /api/wallet/withdraw ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct WithdrawPayload {
    pub amount_cents: i64,
    pub note: Option<String>,
}

pub async fn wallet_withdraw(
    State(st): State<WalletState>,
    cookies: Cookies,
    Json(p): Json<WithdrawPayload>,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    if p.amount_cents < MIN_WITHDRAWAL_CENTS {
        return Json(json!({
            "ok": false,
            "error": format!("minimum withdrawal is {}", cents_to_display(MIN_WITHDRAWAL_CENTS))
        }));
    }

    let mut tx = match st.pool.begin().await {
        Ok(t) => t,
        Err(e) => return Json(json!({"ok": false, "error": format!("begin tx: {e}")})),
    };

    let bal_row = sqlx::query("SELECT balance_cents FROM users WHERE id = $1 FOR UPDATE")
        .bind(&uid)
        .fetch_optional(&mut *tx)
        .await;

    let bal: i64 = match bal_row {
        Ok(Some(r)) => r.try_get("balance_cents").unwrap_or(0),
        _ => {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": "user not found"}));
        }
    };

    if bal < p.amount_cents {
        let _ = tx.rollback().await;
        return Json(json!({
            "ok": false,
            "error": format!("insufficient balance ({})", cents_to_display(bal))
        }));
    }

    let new_bal = bal - p.amount_cents;

    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(new_bal)
        .bind(&uid)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db update: {e}")}));
    }

    let ins = sqlx::query(
        r#"INSERT INTO wallet_transactions
               (user_id, txn_type, amount_cents, balance_after, status, note)
           VALUES ($1, 'withdrawal', $2, $3, 'pending', $4)
           RETURNING id"#,
    )
    .bind(&uid)
    .bind(p.amount_cents)
    .bind(new_bal)
    .bind(p.note.as_deref())
    .fetch_one(&mut *tx)
    .await;

    match ins {
        Ok(r) => {
            let id: i64 = r.try_get("id").unwrap_or(0);
            let _ = tx.commit().await;
            Json(json!({
                "ok": true,
                "txn_id": id,
                "balance_cents": new_bal,
                "balance_display": cents_to_display(new_bal),
                "message": "Withdrawal request submitted. Admin will process your payout."
            }))
        }
        Err(e) => {
            let _ = tx.rollback().await;
            Json(json!({"ok": false, "error": format!("db insert: {e}")}))
        }
    }
}

// ─── POST /api/wallet/transfer ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TransferPayload {
    pub to_username: String,
    pub amount_cents: i64,
    pub note: Option<String>,
}

pub async fn wallet_transfer(
    State(st): State<WalletState>,
    cookies: Cookies,
    Json(p): Json<TransferPayload>,
) -> impl IntoResponse {
    let payment_settings = load_payment_settings(&st.pool).await;
    if !payment_settings.wallet_transfer_enabled {
        return Json(
            json!({"ok": false, "error": "wallet transfer is currently disabled by admin"}),
        );
    }

    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    if p.amount_cents < MIN_TRANSFER_CENTS {
        return Json(json!({
            "ok": false,
            "error": format!("minimum transfer is {}", cents_to_display(MIN_TRANSFER_CENTS))
        }));
    }

    let recip_row = sqlx::query("SELECT id, username FROM users WHERE username = $1 LIMIT 1")
        .bind(p.to_username.trim())
        .fetch_optional(&st.pool)
        .await;

    let recip_row = match recip_row {
        Ok(Some(r)) => r,
        Ok(None) => return Json(json!({"ok": false, "error": "recipient not found"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let recip_id: String = recip_row.try_get("id").unwrap_or_default();
    let recip_name: String = recip_row.try_get("username").unwrap_or_default();

    if recip_id == uid {
        return Json(json!({"ok": false, "error": "cannot transfer to yourself"}));
    }

    let mut tx = match st.pool.begin().await {
        Ok(t) => t,
        Err(e) => return Json(json!({"ok": false, "error": format!("begin tx: {e}")})),
    };

    // Lock both rows in deterministic order to prevent deadlock
    let (id_a, id_b) = if uid < recip_id {
        (uid.clone(), recip_id.clone())
    } else {
        (recip_id.clone(), uid.clone())
    };
    let _ = sqlx::query("SELECT id FROM users WHERE id IN ($1, $2) ORDER BY id FOR UPDATE")
        .bind(&id_a)
        .bind(&id_b)
        .fetch_all(&mut *tx)
        .await;

    let sender_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&uid)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    if sender_bal < p.amount_cents {
        let _ = tx.rollback().await;
        return Json(json!({
            "ok": false,
            "error": format!("insufficient balance ({})", cents_to_display(sender_bal))
        }));
    }

    let recip_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&recip_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    let sender_new = sender_bal - p.amount_cents;
    let recip_new = recip_bal + p.amount_cents;

    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(sender_new)
        .bind(&uid)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db sender: {e}")}));
    }

    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(recip_new)
        .bind(&recip_id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db recipient: {e}")}));
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO wallet_transactions (user_id,txn_type,amount_cents,balance_after,status,ref_user_id,note) VALUES ($1,'transfer_out',$2,$3,'completed',$4,$5)"
    ).bind(&uid).bind(p.amount_cents).bind(sender_new).bind(&recip_id).bind(p.note.as_deref())
    .execute(&mut *tx).await
    { let _ = tx.rollback().await; return Json(json!({"ok": false, "error": format!("ledger out: {e}")})); }

    if let Err(e) = sqlx::query(
        "INSERT INTO wallet_transactions (user_id,txn_type,amount_cents,balance_after,status,ref_user_id,note) VALUES ($1,'transfer_in',$2,$3,'completed',$4,$5)"
    ).bind(&recip_id).bind(p.amount_cents).bind(recip_new).bind(&uid).bind(p.note.as_deref())
    .execute(&mut *tx).await
    { let _ = tx.rollback().await; return Json(json!({"ok": false, "error": format!("ledger in: {e}")})); }

    match tx.commit().await {
        Ok(_) => Json(json!({
            "ok": true,
            "balance_cents":   sender_new,
            "balance_display": cents_to_display(sender_new),
            "transferred_to":  recip_name,
            "amount_display":  cents_to_display(p.amount_cents)
        })),
        Err(e) => Json(json!({"ok": false, "error": format!("commit: {e}")})),
    }
}

// ─── POST /api/wallet/pay ─────────────────────────────────────────────────────
// Buy a video using internal wallet balance.
// Creator receives `creator_split_bp / 10000` of the price into their balance.
// Platform retains the remainder as revenue (not credited to any user balance).

#[derive(Deserialize)]
pub struct WalletPayPayload {
    pub video_id: String,
    pub ref_code: Option<String>, // affiliate referral username
}

pub async fn wallet_pay_video(
    State(st): State<WalletState>,
    cookies: Cookies,
    Json(p): Json<WalletPayPayload>,
) -> impl IntoResponse {
    let payment_settings = load_payment_settings(&st.pool).await;
    if !payment_settings.wallet_payment_enabled {
        return Json(
            json!({"ok": false, "error": "wallet payment is currently disabled by admin"}),
        );
    }

    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    if p.video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    // Load video and creator info
    let video_row = sqlx::query(
        "SELECT v.price_cents, v.owner_id, u.username AS owner_username \
         FROM videos v JOIN users u ON u.id = v.owner_id WHERE v.id = $1 LIMIT 1",
    )
    .bind(&p.video_id)
    .fetch_optional(&st.pool)
    .await;

    let video_row = match video_row {
        Ok(Some(r)) => r,
        Ok(None) => return Json(json!({"ok": false, "error": "video not found"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let price_cents: i64 = video_row.try_get("price_cents").unwrap_or(0);
    let owner_id: String = video_row.try_get("owner_id").unwrap_or_default();
    let owner_username: String = video_row.try_get("owner_username").unwrap_or_default();

    if owner_id == uid {
        return Json(json!({"ok": false, "error": "you own this video"}));
    }
    if price_cents <= 0 {
        return Json(json!({"ok": false, "error": "video has no price set"}));
    }

    // Check already purchased
    let already: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM purchases WHERE user_id = $1 AND video_id = $2")
            .bind(&uid)
            .bind(&p.video_id)
            .fetch_one(&st.pool)
            .await
            .unwrap_or(0);

    if already > 0 {
        return Json(json!({"ok": false, "error": "already purchased"}));
    }

    // Resolve buyer username for allowlist
    let buyer_username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(&uid)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    // Creator split (basis points, e.g. 9000 = 90%)
    let creator_cut =
        (price_cents as i128).saturating_mul(st.cfg.creator_split_bp as i128) / 10_000;
    let creator_cut = creator_cut as i64;

    let mut tx = match st.pool.begin().await {
        Ok(t) => t,
        Err(e) => return Json(json!({"ok": false, "error": format!("begin tx: {e}")})),
    };

    // Lock buyer + creator rows in deterministic order
    let (id_a, id_b) = if uid < owner_id {
        (uid.clone(), owner_id.clone())
    } else {
        (owner_id.clone(), uid.clone())
    };
    let _ = sqlx::query("SELECT id FROM users WHERE id IN ($1, $2) ORDER BY id FOR UPDATE")
        .bind(&id_a)
        .bind(&id_b)
        .fetch_all(&mut *tx)
        .await;

    // Check buyer balance
    let buyer_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&uid)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    if buyer_bal < price_cents {
        let _ = tx.rollback().await;
        return Json(json!({
            "ok": false,
            "error": format!("insufficient balance (have {}, need {})",
                cents_to_display(buyer_bal), cents_to_display(price_cents))
        }));
    }

    // Read creator balance
    let creator_bal: i64 = sqlx::query("SELECT balance_cents FROM users WHERE id = $1")
        .bind(&owner_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()
        .flatten()
        .map(|r: sqlx::postgres::PgRow| r.try_get("balance_cents").unwrap_or(0))
        .unwrap_or(0);

    let buyer_new = buyer_bal - price_cents;
    let creator_new = creator_bal + creator_cut;

    // Deduct buyer
    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(buyer_new)
        .bind(&uid)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db buyer: {e}")}));
    }

    // Credit creator
    if let Err(e) = sqlx::query("UPDATE users SET balance_cents = $1 WHERE id = $2")
        .bind(creator_new)
        .bind(&owner_id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db creator: {e}")}));
    }

    // Ledger: buyer side (payment)
    if let Err(e) = sqlx::query(
        "INSERT INTO wallet_transactions (user_id,txn_type,amount_cents,balance_after,status,ref_user_id,note) \
         VALUES ($1,'payment',$2,$3,'completed',$4,$5)"
    )
    .bind(&uid).bind(price_cents).bind(buyer_new).bind(&owner_id)
    .bind(format!("Video purchase: {}", p.video_id))
    .execute(&mut *tx).await
    { let _ = tx.rollback().await; return Json(json!({"ok": false, "error": format!("ledger buyer: {e}")})); }

    // Ledger: creator side (transfer_in from video sale)
    if let Err(e) = sqlx::query(
        "INSERT INTO wallet_transactions (user_id,txn_type,amount_cents,balance_after,status,ref_user_id,note) \
         VALUES ($1,'transfer_in',$2,$3,'completed',$4,$5)"
    )
    .bind(&owner_id).bind(creator_cut).bind(creator_new).bind(&uid)
    .bind(format!("Video sale: {}", p.video_id))
    .execute(&mut *tx).await
    { let _ = tx.rollback().await; return Json(json!({"ok": false, "error": format!("ledger creator: {e}")})); }

    // Purchase record
    if let Err(e) = sqlx::query(
        "INSERT INTO purchases (user_id, video_id, created_at) VALUES ($1,$2,NOW()) ON CONFLICT DO NOTHING"
    )
    .bind(&uid).bind(&p.video_id).execute(&mut *tx).await
    { let _ = tx.rollback().await; return Json(json!({"ok": false, "error": format!("purchase: {e}")})); }

    // Allowlist
    if !buyer_username.is_empty() {
        if let Err(e) = sqlx::query(
            "INSERT INTO allowlist (video_id, username) VALUES ($1,$2) ON CONFLICT (video_id,username) DO NOTHING"
        )
        .bind(&p.video_id).bind(&buyer_username).execute(&mut *tx).await
        { let _ = tx.rollback().await; return Json(json!({"ok": false, "error": format!("allowlist: {e}")})); }
    }

    match tx.commit().await {
        Ok(_) => {
            // Best-effort affiliate commission (separate tx; does not fail the purchase)
            if let Some(ref_username) = p.ref_code.as_deref().filter(|s| !s.is_empty()) {
                if let Err(e) = commission::process_affiliate_commission(
                    &st.pool,
                    &p.video_id,
                    &uid,
                    &owner_id,
                    price_cents,
                    ref_username,
                    "wallet",
                    None,
                )
                .await
                {
                    tracing::warn!("affiliate commission skipped: {e}");
                }
            }
            Json(json!({
                "ok": true,
                "balance_cents":   buyer_new,
                "balance_display": cents_to_display(buyer_new),
                "creator_received_display": cents_to_display(creator_cut),
                "paid_display": cents_to_display(price_cents),
                "message": format!("Access granted. {} sent to @{}.", cents_to_display(creator_cut), owner_username)
            }))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("commit: {e}")})),
    }
}
