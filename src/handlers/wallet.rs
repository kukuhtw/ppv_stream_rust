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
        })
        .collect();

    Json(json!({"ok": true, "items": items}))
}

// ─── POST /api/wallet/deposit ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DepositPayload {
    pub amount_cents: i64,
    pub note: Option<String>,
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
            Json(json!({
                "ok": true,
                "txn_id": id,
                "message": "Deposit request submitted. Awaiting admin approval."
            }))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
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
