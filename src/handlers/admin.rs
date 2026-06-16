// src/handlers/admin.rs
use crate::payment_settings::load_payment_settings;
use crate::plugins::payment::PaymentPluginRegistry;
use crate::storage_settings::{collect_local_files, load_storage_settings, StoredStorageSettings};
use crate::{config::Config, sessions};
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse},
    Json,
};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::path::Path as FsPath;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tower_cookies::Cookies;
use tracing::{info, warn};
use uuid::Uuid;

static STORAGE_MIGRATION_RUNNING: AtomicBool = AtomicBool::new(false);
static STORAGE_MIGRATION_CANCELLED: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));
const STORAGE_MIGRATION_MAX_ATTEMPTS: usize = 3;
const STORAGE_MIGRATION_MAX_RETRIES: usize = STORAGE_MIGRATION_MAX_ATTEMPTS - 1;
const STORAGE_MIGRATION_RETRY_DELAYS_MS: [u64; 2] = [750, 2000];

#[derive(Clone)]
pub struct AdminState {
    pub pool: crate::db::PgPool,
    pub cfg: Config,
}

async fn ensure_admin_session(
    st: &AdminState,
    cookies: &Cookies,
) -> Result<String, Json<serde_json::Value>> {
    match sessions::current_user_id(&st.pool, &st.cfg, cookies).await {
        Some((user_id, true)) => Ok(user_id),
        Some(_) => Err(Json(json!({"ok": false, "error": "admin only"}))),
        None => Err(Json(json!({"ok": false, "error": "not logged in"}))),
    }
}

// (asumsikan kamu sudah punya handler admin_dashboard & admin_stats di file ini)

#[derive(Deserialize)]
pub struct AdminListQuery {
    pub limit: Option<usize>,
}

pub async fn admin_data(
    State(st): State<AdminState>,
    cookies: Cookies,
    Query(q): Query<AdminListQuery>,
) -> Html<String> {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(_) => return Html("<h1>Forbidden</h1><p>Admin only</p>".into()),
    };
    info!(admin_user_id = %admin_user_id, action = "admin_data_view", limit = q.limit.unwrap_or(50).min(500), "admin data viewed");

    let limit = q.limit.unwrap_or(50).min(500); // batas aman

    // Helper escape HTML
    fn esc(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    // ---- Ambil data per tabel ----
    // users
    let users = sqlx::query(
        r#"SELECT id, username, is_admin, created_at
           FROM users
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit as i64)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // sessions
    let sessions_rows = sqlx::query(
        r#"SELECT id, user_id, is_admin, created_at, expires_at
           FROM sessions
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit as i64)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // videos
    let videos = sqlx::query(
        r#"SELECT id, owner_id, title, price_cents, filename, created_at
           FROM videos
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit as i64)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // allowlist
    let allowlist = sqlx::query(
        r#"SELECT video_id, username
           FROM allowlist
           ORDER BY video_id, username
           LIMIT $1"#,
    )
    .bind(limit as i64)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // purchases
    let purchases = sqlx::query(
        r#"SELECT id, user_id, video_id, created_at
           FROM purchases
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit as i64)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // password_resets
    let resets = sqlx::query(
        r#"SELECT id, user_id, expires_at, used, created_at
           FROM password_resets
           ORDER BY created_at DESC
           LIMIT $1"#,
    )
    .bind(limit as i64)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // ---- Hitung total per tabel (opsional, cepat & informatif) ----
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_videos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM videos")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_allowlist: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM allowlist")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_purchases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM purchases")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_resets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM password_resets")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);

    // ---- Builder HTML ----
    let mut html = String::new();
    html.push_str(r#"<!doctype html><meta charset="utf-8"><title>Admin Data</title>
<style>
  :root{color-scheme:light dark;}
  body{font-family:system-ui,-apple-system,Segoe UI,Roboto,Ubuntu,Arial,sans-serif;margin:20px}
  h1{margin:0 0 16px 0}
  .meta{margin-bottom:12px;color:#666;font-size:14px}
  .card{background:rgba(0,0,0,.03);border:1px solid rgba(0,0,0,.1);padding:14px;border-radius:10px;margin:18px 0}
  .card h2{margin:0 0 10px 0;font-size:18px}
  table{width:100%;border-collapse:collapse;font-size:14px}
  th, td{border:1px solid rgba(0,0,0,.15);padding:6px 8px;vertical-align:top}
  th{background:rgba(0,0,0,.05);text-align:left}
  .mono{font-family:ui-monospace, SFMono-Regular, Menlo, Consolas, monospace}
  .caps{font-variant:all-small-caps;letter-spacing:.04em}
</style>
<h1>Admin Data</h1>
<div class="meta">Menampilkan hingga <span class="mono">limit</span> baris per tabel. Ubah limit pakai query, contoh: <span class="mono">/admin/data?limit=100</span></div>
"#);

    // users
    html.push_str(&format!(
        r#"
<div class="card"><h2>users <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>id</th><th>username</th><th>is_admin</th><th>created_at</th></tr></thead>
  <tbody>
"#,
        total_users
    ));

    for r in &users {
        let id: String = r.get("id");
        let username: String = r.get("username");
        let is_admin: i32 = r.get("is_admin");
        let created_at: String = r.get("created_at");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td>{}</td><td>{}</td><td class='mono'>{}</td></tr>",
            esc(&id),
            esc(&username),
            is_admin,
            esc(&created_at)
        ));
    }
    html.push_str("</tbody></table></div>");

    // sessions
    html.push_str(&format!(r#"
<div class="card"><h2>sessions <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>id</th><th>user_id</th><th>is_admin</th><th>created_at</th><th>expires_at</th></tr></thead>
  <tbody>
"#, total_sessions));

    for r in &sessions_rows {
        let id: String = r.get("id");
        let user_id: Option<String> = r.try_get("user_id").ok();
        let is_admin: i32 = r.get("is_admin");
        let created_at: String = r.get("created_at");
        let expires_at: String = r.get("expires_at");
        let redacted_id = if id.len() > 8 {
            format!("{}...", &id[..8])
        } else {
            id.clone()
        };
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td class='mono'>{}</td><td>{}</td><td class='mono'>{}</td><td class='mono'>{}</td></tr>",
            esc(&redacted_id), esc(user_id.as_deref().unwrap_or("")),
            is_admin, esc(&created_at), esc(&expires_at)
        ));
    }
    html.push_str("</tbody></table></div>");

    // videos
    html.push_str(&format!(r#"
<div class="card"><h2>videos <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>id</th><th>owner_id</th><th>title</th><th>price_cents</th><th>filename</th><th>created_at</th></tr></thead>
  <tbody>
"#, total_videos));

    for r in &videos {
        let id: String = r.get("id");
        let owner_id: String = r.get("owner_id");
        let title: String = r.get("title");
        let price_cents: i32 = r.get("price_cents");
        let filename: String = r.get("filename");
        let created_at: String = r.get("created_at");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td class='mono'>{}</td><td>{}</td><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td></tr>",
            esc(&id), esc(&owner_id), esc(&title), price_cents, esc(&filename), esc(&created_at)
        ));
    }
    html.push_str("</tbody></table></div>");

    // allowlist
    html.push_str(&format!(
        r#"
<div class="card"><h2>allowlist <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>video_id</th><th>username</th></tr></thead>
  <tbody>
"#,
        total_allowlist
    ));

    for r in &allowlist {
        let video_id: String = r.get("video_id");
        let username: String = r.get("username");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td>{}</td></tr>",
            esc(&video_id),
            esc(&username)
        ));
    }
    html.push_str("</tbody></table></div>");

    // purchases
    html.push_str(&format!(
        r#"
<div class="card"><h2>purchases <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>id</th><th>user_id</th><th>video_id</th><th>created_at</th></tr></thead>
  <tbody>
"#,
        total_purchases
    ));

    for r in &purchases {
        let id: String = r.get("id");
        let user_id: String = r.get("user_id");
        let video_id: String = r.get("video_id");
        let created_at: String = r.get("created_at");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td></tr>",
            esc(&id), esc(&user_id), esc(&video_id), esc(&created_at)
        ));
    }
    html.push_str("</tbody></table></div>");

    // password_resets
    html.push_str(&format!(r#"
<div class="card"><h2>password_resets <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>id</th><th>user_id</th><th>token</th><th>expires_at</th><th>used</th><th>created_at</th></tr></thead>
  <tbody>
"#, total_resets));

    for r in &resets {
        let id: i32 = r.get("id"); // SERIAL
        let user_id: String = r.get("user_id");
        let expires_at: String = r.get("expires_at");
        let used: i32 = r.get("used");
        let created_at: String = r.get("created_at");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td><td>{}</td><td class='mono'>{}</td></tr>",
            id, esc(&user_id), "[redacted]", esc(&expires_at), used, esc(&created_at)
        ));
    }
    html.push_str("</tbody></table></div>");

    Html(html)
}

// ---------------------------------------------------------------------------
// Payment monitoring — GET /admin/payments?provider=&status=&limit=
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PaymentsQuery {
    pub provider: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

pub async fn admin_payments(
    State(st): State<AdminState>,
    cookies: Cookies,
    Query(q): Query<PaymentsQuery>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let limit = q.limit.unwrap_or(100).min(1000);
    let provider = q.provider.as_deref().unwrap_or("");
    let status = q.status.as_deref().unwrap_or("");
    info!(admin_user_id = %admin_user_id, action = "admin_payments_view", provider = provider, status = status, limit = limit, "admin payments viewed");

    // Build WHERE clauses dynamically using runtime query (not macro) to avoid DB-at-build-time
    let base_sql = r#"
        SELECT
            fi.invoice_uid,
            fi.provider,
            fi.status,
            fi.amount,
            fi.currency,
            fi.payment_url,
            fi.created_at::TEXT    AS created_at,
            fi.paid_at::TEXT       AS paid_at,
            fi.disbursed_at::TEXT  AS disbursed_at,
            fi.disburse_ref,
            buyer.username         AS buyer_username,
            v.title                AS video_title,
            creator.username       AS creator_username,
            creator.bank_account   AS creator_bank
        FROM fiat_invoices fi
        JOIN users  buyer   ON buyer.id   = fi.user_id
        JOIN videos v       ON v.id       = fi.video_id
        JOIN users  creator ON creator.id = fi.creator_id
    "#;

    let mut conditions: Vec<String> = vec![];
    if !provider.is_empty() {
        conditions.push(format!("fi.provider = '{}'", provider.replace('\'', "''")));
    }
    if !status.is_empty() {
        conditions.push(format!("fi.status = '{}'", status.replace('\'', "''")));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("{base_sql} {where_clause} ORDER BY fi.created_at DESC LIMIT {limit}");

    let rows = sqlx::query(&sql)
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default();

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "invoice_uid":      r.try_get::<String, _>("invoice_uid").unwrap_or_default(),
                "provider":         r.try_get::<String, _>("provider").unwrap_or_default(),
                "status":           r.try_get::<String, _>("status").unwrap_or_default(),
                "amount":           r.try_get::<i64,   _>("amount").unwrap_or(0),
                "currency":         r.try_get::<String, _>("currency").unwrap_or_default(),
                "payment_url":      r.try_get::<Option<String>, _>("payment_url").unwrap_or(None),
                "created_at":       r.try_get::<Option<String>, _>("created_at").unwrap_or(None),
                "paid_at":          r.try_get::<Option<String>, _>("paid_at").unwrap_or(None),
                "disbursed_at":     r.try_get::<Option<String>, _>("disbursed_at").unwrap_or(None),
                "disburse_ref":     r.try_get::<Option<String>, _>("disburse_ref").unwrap_or(None),
                "buyer_username":   r.try_get::<String, _>("buyer_username").unwrap_or_default(),
                "video_title":      r.try_get::<Option<String>, _>("video_title").unwrap_or(None),
                "creator_username": r.try_get::<String, _>("creator_username").unwrap_or_default(),
                "creator_bank":     r.try_get::<Option<String>, _>("creator_bank").unwrap_or(None),
            })
        })
        .collect();

    // Totals for filter status badge counts
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_paid: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices WHERE status='paid'")
            .fetch_one(&st.pool)
            .await
            .unwrap_or(0);
    let total_pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices WHERE status='pending'")
            .fetch_one(&st.pool)
            .await
            .unwrap_or(0);
    let total_failed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiat_invoices WHERE status IN ('failed','expired','cancelled')",
    )
    .fetch_one(&st.pool)
    .await
    .unwrap_or(0);

    Json(json!({
        "ok": true,
        "totals": {
            "all":     total,
            "paid":    total_paid,
            "pending": total_pending,
            "failed":  total_failed,
        },
        "items": items
    }))
}

// ---------------------------------------------------------------------------
// Manual disburse — POST /admin/payments/:uid/disburse
// ---------------------------------------------------------------------------
//
// For Xendit: triggers the Disbursement API (same as the auto-webhook path).
// For Stripe / PayPal / Midtrans: marks the record as disbursed (admin confirms
//   they have already transferred via the provider's own dashboard).

pub async fn admin_disburse(
    State(st): State<AdminState>,
    cookies: Cookies,
    Path(uid): Path<String>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    // Fetch invoice + creator bank_account in one query
    let row = sqlx::query(
        r#"SELECT fi.provider, fi.amount, fi.currency, fi.status, fi.disbursed_at,
                  creator.bank_account AS creator_bank
           FROM fiat_invoices fi
           JOIN users creator ON creator.id = fi.creator_id
           WHERE fi.invoice_uid = $1"#,
    )
    .bind(&uid)
    .fetch_optional(&st.pool)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return Json(json!({"ok": false, "error": "invoice not found"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let status: String = row.try_get("status").unwrap_or_default();
    let disbursed_at: Option<String> = row.try_get("disbursed_at").unwrap_or(None);
    let provider: String = row.try_get("provider").unwrap_or_default();
    let amount: i64 = row.try_get("amount").unwrap_or(0);

    if status != "paid" {
        return Json(json!({"ok": false, "error": "invoice is not paid yet"}));
    }
    if disbursed_at.is_some() {
        return Json(json!({"ok": false, "error": "already disbursed"}));
    }

    if provider == "xendit" {
        // Try actual Xendit disbursement
        use crate::plugins::payment::providers::xendit::XenditPaymentPlugin;
        let creator_bank: Option<String> = row.try_get("creator_bank").unwrap_or(None);

        match creator_bank {
            Some(ba) if !ba.trim().is_empty() => {
                let xp = XenditPaymentPlugin::from_env();
                match xp.disburse_to_creator(&ba, amount, &uid).await {
                    Ok(resp) => {
                        let disburse_ref = resp["id"].as_str().unwrap_or("").to_string();
                        let _ = sqlx::query(
                            "UPDATE fiat_invoices SET disbursed_at = now(), disburse_ref = $1 WHERE invoice_uid = $2"
                        )
                    .bind(&disburse_ref)
                    .bind(&uid)
                    .execute(&st.pool)
                    .await;
                        info!(admin_user_id = %admin_user_id, action = "admin_disburse", provider = %provider, invoice_uid = %uid, amount_cents = amount, method = "xendit_api", disburse_ref = %disburse_ref, "manual admin disbursement executed");
                        return Json(
                            json!({"ok": true, "disburse_ref": disburse_ref, "method": "xendit_api"}),
                        );
                    }
                    Err(e) => {
                        warn!(admin_user_id = %admin_user_id, action = "admin_disburse_failed", provider = %provider, invoice_uid = %uid, amount_cents = amount, error = %e, "admin disbursement failed");
                        return Json(
                            json!({"ok": false, "error": format!("xendit disburse: {e}")}),
                        );
                    }
                }
            }
            _ => {
                return Json(json!({"ok": false, "error": "creator has no bank_account set"}));
            }
        }
    }

    // For all other providers: admin confirms manual disbursement
    let _ = sqlx::query(
        "UPDATE fiat_invoices SET disbursed_at = now(), disburse_ref = 'manual' WHERE invoice_uid = $1"
    )
    .bind(&uid)
    .execute(&st.pool)
    .await;
    info!(admin_user_id = %admin_user_id, action = "admin_disburse", provider = %provider, invoice_uid = %uid, amount_cents = amount, method = "manual", "manual disbursement marked");

    Json(json!({"ok": true, "method": "manual", "invoice_uid": uid}))
}

// ---------------------------------------------------------------------------
// SMTP settings — GET /admin/smtp  /  POST /admin/smtp
// ---------------------------------------------------------------------------

pub async fn admin_smtp_get(State(st): State<AdminState>, cookies: Cookies) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };
    info!(admin_user_id = %admin_user_id, action = "admin_smtp_view", "smtp settings viewed");

    let row = sqlx::query(
        "SELECT host, port, username, password, from_email, from_name, use_tls, enabled
         FROM smtp_settings WHERE id = 1",
    )
    .fetch_optional(&st.pool)
    .await;

    match row {
        Ok(Some(r)) => Json(json!({
            "ok": true,
            "smtp": {
                "host":       r.try_get::<String,  _>("host").unwrap_or_default(),
                "port":       r.try_get::<i32,     _>("port").unwrap_or(587),
                "username":   r.try_get::<String,  _>("username").unwrap_or_default(),
                "password":   "",
                "has_password": r.try_get::<String, _>("password").map(|v| !v.is_empty()).unwrap_or(false),
                "from_email": r.try_get::<String,  _>("from_email").unwrap_or_default(),
                "from_name":  r.try_get::<String,  _>("from_name").unwrap_or_else(|_| "PPV Stream".into()),
                "use_tls":    r.try_get::<bool,    _>("use_tls").unwrap_or(true),
                "enabled":    r.try_get::<bool,    _>("enabled").unwrap_or(false),
            }
        })),
        _ => Json(json!({"ok": false, "error": "smtp_settings not found"})),
    }
}

#[derive(Deserialize)]
pub struct SmtpSavePayload {
    pub host: String,
    pub port: i32,
    pub username: String,
    pub password: String,
    pub from_email: String,
    pub from_name: String,
    pub use_tls: bool,
    pub enabled: bool,
    /// Optional: send a test email to this address after saving
    pub test_email: Option<String>,
}

#[derive(Deserialize)]
pub struct PaymentSettingsSavePayload {
    pub wallet_payment_enabled: bool,
    pub wallet_transfer_enabled: bool,
    pub paypal_enabled: bool,
    pub stripe_enabled: bool,
    pub xendit_enabled: bool,
    pub midtrans_enabled: bool,
    pub x402_enabled: bool,
    pub default_provider: Option<String>,
}

#[derive(Deserialize)]
pub struct StorageSettingsSavePayload {
    pub backend: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub endpoint: String,
    pub public_url: String,
    pub path_style: bool,
}

#[derive(Deserialize)]
pub struct StorageMigrationStartPayload {
    pub include_uploads: bool,
    pub include_media: bool,
    pub resume_from_job_id: Option<String>,
}

async fn test_storage_settings_connection(
    cfg: &Config,
    settings: &StoredStorageSettings,
) -> anyhow::Result<()> {
    let storage = settings.build_plugin()?;
    if settings.normalized_backend() == "local" {
        return Ok(());
    }

    let temp_name = format!("storage-test-{}.txt", Uuid::new_v4());
    let temp_path = FsPath::new(&cfg.tmp_dir).join(&temp_name);
    if let Some(parent) = temp_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&temp_path, b"ppv-stream storage connection test").await?;

    let key = format!("healthchecks/{temp_name}");
    let put_result = storage.put_file(&key, &temp_path).await;
    let _ = tokio::fs::remove_file(&temp_path).await;
    put_result?;
    storage.delete(&key).await?;
    Ok(())
}

fn active_storage_summary() -> serde_json::Value {
    let backend = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "local".into());
    json!({
        "backend": backend,
        "bucket": std::env::var("S3_BUCKET").unwrap_or_default(),
        "region": std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
        "endpoint": std::env::var("S3_ENDPOINT").unwrap_or_default(),
        "public_url": std::env::var("S3_PUBLIC_URL").unwrap_or_default(),
        "path_style": std::env::var("S3_PATH_STYLE").ok().as_deref() == Some("true"),
    })
}

async fn list_storage_migration_jobs_json(pool: &crate::db::PgPool) -> Vec<serde_json::Value> {
    let rows = sqlx::query(
        r#"SELECT id, status, backend, bucket, endpoint, include_uploads, include_media,
                  total_files, copied_files, failed_files, skipped_files, retry_attempts, last_error,
                  resumed_from_job_id,
                  started_by_user_id, created_at::TEXT AS created_at,
                  started_at::TEXT AS started_at, completed_at::TEXT AS completed_at
           FROM storage_migration_jobs
           ORDER BY created_at DESC
           LIMIT 20"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").unwrap_or_default(),
                "status": r.try_get::<String, _>("status").unwrap_or_default(),
                "backend": r.try_get::<String, _>("backend").unwrap_or_default(),
                "bucket": r.try_get::<String, _>("bucket").unwrap_or_default(),
                "endpoint": r.try_get::<String, _>("endpoint").unwrap_or_default(),
                "include_uploads": r.try_get::<bool, _>("include_uploads").unwrap_or(true),
                "include_media": r.try_get::<bool, _>("include_media").unwrap_or(true),
                "total_files": r.try_get::<i64, _>("total_files").unwrap_or(0),
                "copied_files": r.try_get::<i64, _>("copied_files").unwrap_or(0),
                "failed_files": r.try_get::<i64, _>("failed_files").unwrap_or(0),
                "skipped_files": r.try_get::<i64, _>("skipped_files").unwrap_or(0),
                "retry_attempts": r.try_get::<i64, _>("retry_attempts").unwrap_or(0),
                "last_error": r.try_get::<Option<String>, _>("last_error").unwrap_or(None),
                "resumed_from_job_id": r.try_get::<Option<String>, _>("resumed_from_job_id").unwrap_or(None),
                "started_by_user_id": r.try_get::<Option<String>, _>("started_by_user_id").unwrap_or(None),
                "created_at": r.try_get::<Option<String>, _>("created_at").unwrap_or(None),
                "started_at": r.try_get::<Option<String>, _>("started_at").unwrap_or(None),
                "completed_at": r.try_get::<Option<String>, _>("completed_at").unwrap_or(None),
            })
        })
        .collect()
}

async fn list_storage_migration_job_items_json(
    pool: &crate::db::PgPool,
    job_id: &str,
) -> Vec<serde_json::Value> {
    let rows = sqlx::query(
        r#"SELECT id, scope, source_path, object_key, status, retry_attempts,
                  error_message, created_at::TEXT AS created_at
           FROM storage_migration_job_items
           WHERE job_id = $1
           ORDER BY created_at DESC
           LIMIT 200"#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").unwrap_or_default(),
                "scope": r.try_get::<String, _>("scope").unwrap_or_default(),
                "source_path": r.try_get::<String, _>("source_path").unwrap_or_default(),
                "object_key": r.try_get::<String, _>("object_key").unwrap_or_default(),
                "status": r.try_get::<String, _>("status").unwrap_or_default(),
                "retry_attempts": r.try_get::<i64, _>("retry_attempts").unwrap_or(0),
                "error_message": r.try_get::<Option<String>, _>("error_message").unwrap_or(None),
                "created_at": r.try_get::<Option<String>, _>("created_at").unwrap_or(None),
            })
        })
        .collect()
}

async fn update_storage_job_progress(
    pool: &crate::db::PgPool,
    job_id: &str,
    copied_files: i64,
    failed_files: i64,
    skipped_files: i64,
    retry_attempts: i64,
    last_error: Option<&str>,
) {
    let _ = sqlx::query(
        r#"UPDATE storage_migration_jobs
           SET copied_files = $2,
               failed_files = $3,
               skipped_files = $4,
               retry_attempts = $5,
               last_error = COALESCE($6, last_error)
           WHERE id = $1"#,
    )
    .bind(job_id)
    .bind(copied_files)
    .bind(failed_files)
    .bind(skipped_files)
    .bind(retry_attempts)
    .bind(last_error)
    .execute(pool)
    .await;
}

async fn insert_storage_migration_job_item(
    pool: &crate::db::PgPool,
    job_id: &str,
    scope: &str,
    source_path: &str,
    object_key: &str,
    status: &str,
    retry_attempts: i64,
    error_message: Option<&str>,
) {
    let _ = sqlx::query(
        r#"INSERT INTO storage_migration_job_items
              (id, job_id, scope, source_path, object_key, status, retry_attempts, error_message, created_at)
           VALUES
              ($1, $2, $3, $4, $5, $6, $7, $8, NOW())"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(job_id)
    .bind(scope)
    .bind(source_path)
    .bind(object_key)
    .bind(status)
    .bind(retry_attempts)
    .bind(error_message)
    .execute(pool)
    .await;
}

async fn load_resumable_storage_object_keys(
    pool: &crate::db::PgPool,
    job_id: &str,
) -> BTreeSet<String> {
    let rows = sqlx::query(
        r#"SELECT object_key
           FROM storage_migration_job_items
           WHERE job_id = $1
             AND status = 'copied'"#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter_map(|row| row.try_get::<String, _>("object_key").ok())
        .collect()
}

async fn request_storage_job_cancel(job_id: &str) {
    let mut cancelled = STORAGE_MIGRATION_CANCELLED.lock().await;
    cancelled.insert(job_id.to_string());
}

async fn clear_storage_job_cancel(job_id: &str) {
    let mut cancelled = STORAGE_MIGRATION_CANCELLED.lock().await;
    cancelled.remove(job_id);
}

async fn is_storage_job_cancelled(job_id: &str) -> bool {
    let cancelled = STORAGE_MIGRATION_CANCELLED.lock().await;
    cancelled.contains(job_id)
}

async fn mark_storage_job_cancelled(
    pool: &crate::db::PgPool,
    job_id: &str,
    copied_files: i64,
    failed_files: i64,
    skipped_files: i64,
    retry_attempts: i64,
    last_error: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE storage_migration_jobs
           SET status = 'cancelled',
               copied_files = $2,
               failed_files = $3,
               skipped_files = $4,
               retry_attempts = $5,
               last_error = COALESCE($6, last_error),
               completed_at = NOW()
           WHERE id = $1"#,
    )
    .bind(job_id)
    .bind(copied_files)
    .bind(failed_files)
    .bind(skipped_files)
    .bind(retry_attempts)
    .bind(last_error)
    .execute(pool)
    .await?;
    clear_storage_job_cancel(job_id).await;
    Ok(())
}

async fn upload_storage_migration_file(
    storage: &dyn crate::plugins::storage::StoragePlugin,
    key: &str,
    path: &FsPath,
    label: &str,
) -> Result<usize, String> {
    let mut last_error = String::new();
    for attempt in 1..=STORAGE_MIGRATION_MAX_ATTEMPTS {
        match storage.put_file(key, path).await {
            Ok(_) => return Ok(attempt.saturating_sub(1)),
            Err(e) => {
                last_error = format!(
                    "{label} {key} attempt {attempt}/{STORAGE_MIGRATION_MAX_ATTEMPTS}: {e}"
                );
                if attempt < STORAGE_MIGRATION_MAX_ATTEMPTS {
                    sleep(Duration::from_millis(
                        STORAGE_MIGRATION_RETRY_DELAYS_MS[attempt - 1],
                    ))
                    .await;
                }
            }
        }
    }

    Err(last_error)
}

async fn run_storage_migration_job(
    pool: crate::db::PgPool,
    cfg: Config,
    job_id: String,
    settings: StoredStorageSettings,
    include_uploads: bool,
    include_media: bool,
    resume_from_job_id: Option<String>,
) -> anyhow::Result<()> {
    let storage = settings.build_plugin()?;
    let mut upload_files = Vec::new();
    let mut media_files = Vec::new();

    if include_uploads {
        upload_files = collect_local_files(FsPath::new(&cfg.upload_dir)).await?;
    }
    if include_media {
        media_files = collect_local_files(FsPath::new(&cfg.media_dir)).await?;
    }

    let total_files = (upload_files.len() + media_files.len()) as i64;
    sqlx::query(
        r#"UPDATE storage_migration_jobs
           SET status = 'running',
               total_files = $2,
               started_at = NOW(),
               last_error = NULL
           WHERE id = $1"#,
    )
    .bind(&job_id)
    .bind(total_files)
    .execute(&pool)
    .await?;

    let mut copied_files = 0_i64;
    let mut failed_files = 0_i64;
    let mut skipped_files = 0_i64;
    let mut retry_attempts = 0_i64;
    let mut last_error: Option<String> = None;
    let resumable_keys = if let Some(source_job_id) = resume_from_job_id.as_deref() {
        load_resumable_storage_object_keys(&pool, source_job_id).await
    } else {
        BTreeSet::new()
    };

    for path in upload_files {
        if is_storage_job_cancelled(&job_id).await {
            mark_storage_job_cancelled(
                &pool,
                &job_id,
                copied_files,
                failed_files,
                skipped_files,
                retry_attempts,
                last_error.as_deref(),
            )
            .await?;
            return Ok(());
        }
        let rel = path
            .strip_prefix(&cfg.upload_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let key = format!("uploads/{rel}");
        let source_path = path.to_string_lossy().replace('\\', "/");
        if resumable_keys.contains(&key) {
            skipped_files += 1;
            insert_storage_migration_job_item(
                &pool,
                &job_id,
                "uploads",
                &source_path,
                &key,
                "skipped",
                0,
                Some("Skipped because this object was already copied in the source resume job"),
            )
            .await;
            update_storage_job_progress(
                &pool,
                &job_id,
                copied_files,
                failed_files,
                skipped_files,
                retry_attempts,
                last_error.as_deref(),
            )
            .await;
            continue;
        }
        match upload_storage_migration_file(storage.as_ref(), &key, &path, "upload").await {
            Ok(file_retry_attempts) => {
                copied_files += 1;
                retry_attempts += file_retry_attempts as i64;
                insert_storage_migration_job_item(
                    &pool,
                    &job_id,
                    "uploads",
                    &source_path,
                    &key,
                    "copied",
                    file_retry_attempts as i64,
                    None,
                )
                .await;
            }
            Err(e) => {
                failed_files += 1;
                retry_attempts += STORAGE_MIGRATION_MAX_RETRIES as i64;
                insert_storage_migration_job_item(
                    &pool,
                    &job_id,
                    "uploads",
                    &source_path,
                    &key,
                    "failed",
                    STORAGE_MIGRATION_MAX_RETRIES as i64,
                    Some(&e),
                )
                .await;
                last_error = Some(e);
            }
        }
        update_storage_job_progress(
            &pool,
            &job_id,
            copied_files,
            failed_files,
            skipped_files,
            retry_attempts,
            last_error.as_deref(),
        )
        .await;
    }

    for path in media_files {
        if is_storage_job_cancelled(&job_id).await {
            mark_storage_job_cancelled(
                &pool,
                &job_id,
                copied_files,
                failed_files,
                skipped_files,
                retry_attempts,
                last_error.as_deref(),
            )
            .await?;
            return Ok(());
        }
        let rel = path
            .strip_prefix(&cfg.media_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let key = format!("videos/{rel}");
        let source_path = path.to_string_lossy().replace('\\', "/");
        if resumable_keys.contains(&key) {
            skipped_files += 1;
            insert_storage_migration_job_item(
                &pool,
                &job_id,
                "media",
                &source_path,
                &key,
                "skipped",
                0,
                Some("Skipped because this object was already copied in the source resume job"),
            )
            .await;
            update_storage_job_progress(
                &pool,
                &job_id,
                copied_files,
                failed_files,
                skipped_files,
                retry_attempts,
                last_error.as_deref(),
            )
            .await;
            continue;
        }
        match upload_storage_migration_file(storage.as_ref(), &key, &path, "media").await {
            Ok(file_retry_attempts) => {
                copied_files += 1;
                retry_attempts += file_retry_attempts as i64;
                insert_storage_migration_job_item(
                    &pool,
                    &job_id,
                    "media",
                    &source_path,
                    &key,
                    "copied",
                    file_retry_attempts as i64,
                    None,
                )
                .await;
            }
            Err(e) => {
                failed_files += 1;
                retry_attempts += STORAGE_MIGRATION_MAX_RETRIES as i64;
                insert_storage_migration_job_item(
                    &pool,
                    &job_id,
                    "media",
                    &source_path,
                    &key,
                    "failed",
                    STORAGE_MIGRATION_MAX_RETRIES as i64,
                    Some(&e),
                )
                .await;
                last_error = Some(e);
            }
        }
        update_storage_job_progress(
            &pool,
            &job_id,
            copied_files,
            failed_files,
            skipped_files,
            retry_attempts,
            last_error.as_deref(),
        )
        .await;
    }

    let final_status = if failed_files > 0 {
        "completed_with_errors"
    } else {
        "completed"
    };
    sqlx::query(
        r#"UPDATE storage_migration_jobs
           SET status = $2,
               copied_files = $3,
               failed_files = $4,
               skipped_files = $5,
               retry_attempts = $6,
               last_error = $7,
               completed_at = NOW()
           WHERE id = $1"#,
    )
    .bind(&job_id)
    .bind(final_status)
    .bind(copied_files)
    .bind(failed_files)
    .bind(skipped_files)
    .bind(retry_attempts)
    .bind(last_error.as_deref())
    .execute(&pool)
    .await?;
    clear_storage_job_cancel(&job_id).await;

    Ok(())
}

pub async fn admin_storage_settings_get(
    State(st): State<AdminState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };
    info!(admin_user_id = %admin_user_id, action = "admin_storage_settings_view", "storage settings viewed");

    let settings = load_storage_settings(&st.pool).await;
    let active = active_storage_summary();
    let missing_fields = settings.missing_fields();
    let desired_backend = settings.normalized_backend();
    let active_backend = active
        .get("backend")
        .and_then(|v| v.as_str())
        .unwrap_or("local")
        .to_ascii_lowercase();

    Json(json!({
        "ok": true,
        "active": active,
        "desired": {
            "backend": settings.backend,
            "bucket": settings.bucket,
            "region": settings.region,
            "access_key": settings.access_key,
            "secret_key": "",
            "has_secret_key": settings.has_secret(),
            "endpoint": settings.endpoint,
            "public_url": settings.public_url,
            "path_style": settings.path_style,
            "missing_fields": missing_fields,
            "configured": missing_fields.is_empty(),
        },
        "restart_required": desired_backend != active_backend,
        "running_job": STORAGE_MIGRATION_RUNNING.load(Ordering::SeqCst),
        "jobs": list_storage_migration_jobs_json(&st.pool).await,
    }))
}

pub async fn admin_storage_settings_save(
    State(st): State<AdminState>,
    cookies: Cookies,
    Json(p): Json<StorageSettingsSavePayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let existing = load_storage_settings(&st.pool).await;
    let settings = StoredStorageSettings {
        backend: p.backend.trim().to_ascii_lowercase(),
        bucket: p.bucket.trim().to_string(),
        region: if p.region.trim().is_empty() {
            "us-east-1".into()
        } else {
            p.region.trim().to_string()
        },
        access_key: p.access_key.trim().to_string(),
        secret_key: if p.secret_key.is_empty() {
            existing.secret_key
        } else {
            p.secret_key
        },
        endpoint: p.endpoint.trim().to_string(),
        public_url: p.public_url.trim().to_string(),
        path_style: p.path_style,
    };

    if let Err(e) = settings.validate() {
        return Json(json!({"ok": false, "error": e.to_string()}));
    }

    let res = sqlx::query(
        r#"INSERT INTO storage_settings
              (id, backend, bucket, region, access_key, secret_key, endpoint, public_url, path_style, updated_at)
           VALUES
              (TRUE, $1, $2, $3, $4, $5, $6, $7, $8, NOW())
           ON CONFLICT (id) DO UPDATE
             SET backend = $1,
                 bucket = $2,
                 region = $3,
                 access_key = $4,
                 secret_key = $5,
                 endpoint = $6,
                 public_url = $7,
                 path_style = $8,
                 updated_at = NOW()"#,
    )
    .bind(&settings.backend)
    .bind(&settings.bucket)
    .bind(&settings.region)
    .bind(&settings.access_key)
    .bind(&settings.secret_key)
    .bind(&settings.endpoint)
    .bind(&settings.public_url)
    .bind(settings.path_style)
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => {
            let restart_required = settings.normalized_backend()
                != std::env::var("STORAGE_BACKEND")
                    .unwrap_or_else(|_| "local".into())
                    .to_ascii_lowercase();
            info!(
                admin_user_id = %admin_user_id,
                action = "admin_storage_settings_save",
                backend = %settings.backend,
                bucket = %settings.bucket,
                endpoint = %settings.endpoint,
                path_style = settings.path_style,
                restart_required = restart_required,
                "storage settings saved"
            );
            Json(json!({
                "ok": true,
                "message": "Storage settings saved. Restart the application to apply the backend change at runtime.",
                "restart_required": restart_required
            }))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn admin_storage_settings_test(
    State(st): State<AdminState>,
    cookies: Cookies,
    Json(p): Json<StorageSettingsSavePayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let existing = load_storage_settings(&st.pool).await;
    let settings = StoredStorageSettings {
        backend: p.backend.trim().to_ascii_lowercase(),
        bucket: p.bucket.trim().to_string(),
        region: if p.region.trim().is_empty() {
            "us-east-1".into()
        } else {
            p.region.trim().to_string()
        },
        access_key: p.access_key.trim().to_string(),
        secret_key: if p.secret_key.is_empty() {
            existing.secret_key
        } else {
            p.secret_key
        },
        endpoint: p.endpoint.trim().to_string(),
        public_url: p.public_url.trim().to_string(),
        path_style: p.path_style,
    };

    if let Err(e) = settings.validate() {
        return Json(json!({"ok": false, "error": e.to_string()}));
    }

    match test_storage_settings_connection(&st.cfg, &settings).await {
        Ok(_) => {
            info!(
                admin_user_id = %admin_user_id,
                action = "admin_storage_settings_test",
                backend = %settings.backend,
                bucket = %settings.bucket,
                endpoint = %settings.endpoint,
                "storage connection test succeeded"
            );
            Json(json!({
                "ok": true,
                "message": if settings.normalized_backend() == "local" {
                    "Local storage configuration is valid."
                } else {
                    "Remote storage connection test succeeded."
                }
            }))
        }
        Err(e) => {
            warn!(
                admin_user_id = %admin_user_id,
                action = "admin_storage_settings_test_failed",
                backend = %settings.backend,
                bucket = %settings.bucket,
                endpoint = %settings.endpoint,
                error = %e,
                "storage connection test failed"
            );
            Json(json!({"ok": false, "error": e.to_string()}))
        }
    }
}

pub async fn admin_storage_migrations_get(
    State(st): State<AdminState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };
    info!(admin_user_id = %admin_user_id, action = "admin_storage_migrations_view", "storage migration jobs viewed");

    Json(json!({
        "ok": true,
        "running_job": STORAGE_MIGRATION_RUNNING.load(Ordering::SeqCst),
        "jobs": list_storage_migration_jobs_json(&st.pool).await,
    }))
}

pub async fn admin_storage_migration_items_get(
    State(st): State<AdminState>,
    cookies: Cookies,
    Path(job_id): Path<String>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };
    info!(
        admin_user_id = %admin_user_id,
        action = "admin_storage_migration_items_view",
        job_id = %job_id,
        "storage migration job items viewed"
    );

    Json(json!({
        "ok": true,
        "job_id": job_id,
        "items": list_storage_migration_job_items_json(&st.pool, &job_id).await,
    }))
}

pub async fn admin_storage_migrations_start(
    State(st): State<AdminState>,
    cookies: Cookies,
    Json(p): Json<StorageMigrationStartPayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    if !p.include_uploads && !p.include_media {
        return Json(json!({"ok": false, "error": "select at least one migration scope"}));
    }

    if STORAGE_MIGRATION_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Json(json!({"ok": false, "error": "a storage migration job is already running"}));
    }

    let settings = load_storage_settings(&st.pool).await;
    if let Err(e) = settings.validate() {
        STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
        return Json(json!({"ok": false, "error": e.to_string()}));
    }
    if settings.normalized_backend() == "local" {
        STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
        return Json(
            json!({"ok": false, "error": "storage migration requires a remote backend such as s3, minio, r2, or b2"}),
        );
    }

    let resume_from_job_id = p
        .resume_from_job_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    if let Some(source_job_id) = resume_from_job_id.as_deref() {
        let resume_row = match sqlx::query(
            r#"SELECT status, backend, include_uploads, include_media
               FROM storage_migration_jobs
               WHERE id = $1"#,
        )
        .bind(source_job_id)
        .fetch_optional(&st.pool)
        .await
        {
            Ok(row) => row,
            Err(e) => {
                STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
                return Json(json!({"ok": false, "error": format!("db: {e}")}));
            }
        };

        let Some(resume_row) = resume_row else {
            STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
            return Json(json!({"ok": false, "error": "resume source job not found"}));
        };

        let resume_status = resume_row
            .try_get::<String, _>("status")
            .unwrap_or_default();
        if !matches!(
            resume_status.as_str(),
            "failed" | "cancelled" | "completed_with_errors"
        ) {
            STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
            return Json(json!({
                "ok": false,
                "error": "resume source job must be failed, cancelled, or completed_with_errors"
            }));
        }

        let resume_backend = resume_row
            .try_get::<String, _>("backend")
            .unwrap_or_default();
        if resume_backend != settings.backend {
            STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
            return Json(json!({
                "ok": false,
                "error": "resume source job must use the same target backend"
            }));
        }
    }

    let job_id = Uuid::new_v4().to_string();
    let insert = sqlx::query(
        r#"INSERT INTO storage_migration_jobs
              (id, status, backend, bucket, endpoint, include_uploads, include_media, resumed_from_job_id, started_by_user_id, created_at)
           VALUES
              ($1, 'pending', $2, $3, $4, $5, $6, $7, $8, NOW())"#,
    )
    .bind(&job_id)
    .bind(&settings.backend)
    .bind(&settings.bucket)
    .bind(&settings.endpoint)
    .bind(p.include_uploads)
    .bind(p.include_media)
    .bind(resume_from_job_id.as_deref())
    .bind(&admin_user_id)
    .execute(&st.pool)
    .await;

    if let Err(e) = insert {
        STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
        return Json(json!({"ok": false, "error": format!("db: {e}")}));
    }

    let pool = st.pool.clone();
    let cfg = st.cfg.clone();
    let job_id_clone = job_id.clone();
    let backend_for_log = settings.backend.clone();
    tokio::spawn(async move {
        let outcome = run_storage_migration_job(
            pool.clone(),
            cfg,
            job_id_clone.clone(),
            settings,
            p.include_uploads,
            p.include_media,
            resume_from_job_id.clone(),
        )
        .await;

        if let Err(e) = outcome {
            let _ = sqlx::query(
                r#"UPDATE storage_migration_jobs
                   SET status = 'failed',
                       last_error = $2,
                       completed_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(&job_id_clone)
            .bind(e.to_string())
            .execute(&pool)
            .await;
            warn!(
                action = "admin_storage_migration_failed",
                job_id = %job_id_clone,
                backend = %backend_for_log,
                error = %e,
                "storage migration failed"
            );
        } else {
            info!(
                action = "admin_storage_migration_completed",
                job_id = %job_id_clone,
                backend = %backend_for_log,
                "storage migration finished"
            );
        }
        clear_storage_job_cancel(&job_id_clone).await;
        STORAGE_MIGRATION_RUNNING.store(false, Ordering::SeqCst);
    });

    info!(
        admin_user_id = %admin_user_id,
        action = "admin_storage_migration_start",
        job_id = %job_id,
        backend = %backend_for_log,
        include_uploads = p.include_uploads,
        include_media = p.include_media,
        resumed_from_job_id = ?resume_from_job_id,
        "storage migration started"
    );

    Json(json!({
        "ok": true,
        "job_id": job_id,
        "resumed_from_job_id": resume_from_job_id,
        "message": "Storage migration started in the background."
    }))
}

pub async fn admin_storage_migration_cancel(
    State(st): State<AdminState>,
    cookies: Cookies,
    Path(job_id): Path<String>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let row = match sqlx::query(
        r#"SELECT status
           FROM storage_migration_jobs
           WHERE id = $1"#,
    )
    .bind(&job_id)
    .fetch_optional(&st.pool)
    .await
    {
        Ok(row) => row,
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let Some(row) = row else {
        return Json(json!({"ok": false, "error": "storage migration job not found"}));
    };

    let status = row.try_get::<String, _>("status").unwrap_or_default();
    if !matches!(status.as_str(), "pending" | "running" | "cancel_requested") {
        return Json(json!({
            "ok": false,
            "error": format!("cannot cancel a job with status '{status}'")
        }));
    }

    request_storage_job_cancel(&job_id).await;
    let update = sqlx::query(
        r#"UPDATE storage_migration_jobs
           SET status = 'cancel_requested',
               last_error = COALESCE(last_error, 'Cancellation requested by admin')
           WHERE id = $1"#,
    )
    .bind(&job_id)
    .execute(&st.pool)
    .await;

    match update {
        Ok(_) => {
            info!(
                admin_user_id = %admin_user_id,
                action = "admin_storage_migration_cancel",
                job_id = %job_id,
                "storage migration cancellation requested"
            );
            Json(json!({
                "ok": true,
                "message": "Cancellation requested. The job will stop after the current file finishes uploading."
            }))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn admin_payment_settings_get(
    State(st): State<AdminState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };
    info!(admin_user_id = %admin_user_id, action = "admin_payment_settings_view", "payment settings viewed");

    let settings = load_payment_settings(&st.pool).await;
    let capabilities =
        PaymentPluginRegistry::capabilities_from_env_with_pool(Some(st.pool.clone()));

    Json(json!({
        "ok": true,
        "settings": settings,
        "providers": capabilities.into_iter().map(|capability| {
            let provider_key = capability.provider.clone();
            json!({
                "provider": capability.provider,
                "display_name": capability.display_name,
                "configured": capability.configured,
                "environment": capability.environment,
                "api_base_url": capability.api_base_url,
                "required_env": capability.required_env,
                "missing_env": capability.missing_env,
                "enabled": settings.is_provider_enabled(&provider_key),
            })
        }).collect::<Vec<_>>()
    }))
}

pub async fn admin_payment_settings_save(
    State(st): State<AdminState>,
    cookies: Cookies,
    Json(p): Json<PaymentSettingsSavePayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let capabilities =
        PaymentPluginRegistry::capabilities_from_env_with_pool(Some(st.pool.clone()));
    let configured = capabilities
        .into_iter()
        .map(|cap| (cap.provider, cap.configured))
        .collect::<std::collections::HashMap<_, _>>();

    let requested = [
        ("paypal", p.paypal_enabled),
        ("stripe", p.stripe_enabled),
        ("xendit", p.xendit_enabled),
        ("midtrans", p.midtrans_enabled),
        ("x402", p.x402_enabled),
    ];

    for (provider, enabled) in requested {
        if enabled && !configured.get(provider).copied().unwrap_or(false) {
            return Json(json!({
                "ok": false,
                "error": format!("{provider} cannot be enabled because its environment variables are incomplete")
            }));
        }
    }

    let default_provider = p
        .default_provider
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    if let Some(provider) = default_provider.as_deref() {
        let enabled = match provider {
            "paypal" => p.paypal_enabled,
            "stripe" => p.stripe_enabled,
            "xendit" => p.xendit_enabled,
            "midtrans" => p.midtrans_enabled,
            "x402" => p.x402_enabled,
            _ => false,
        };
        if !enabled {
            return Json(json!({
                "ok": false,
                "error": "default provider must also be enabled"
            }));
        }
    }

    let res = sqlx::query(
        r#"INSERT INTO payment_settings
              (id, wallet_payment_enabled, wallet_transfer_enabled, paypal_enabled,
               stripe_enabled, xendit_enabled, midtrans_enabled, x402_enabled,
               default_provider, updated_at)
           VALUES
              (TRUE, $1, $2, $3, $4, $5, $6, $7, $8, NOW())
           ON CONFLICT (id) DO UPDATE
             SET wallet_payment_enabled = $1,
                 wallet_transfer_enabled = $2,
                 paypal_enabled = $3,
                 stripe_enabled = $4,
                 xendit_enabled = $5,
                 midtrans_enabled = $6,
                 x402_enabled = $7,
                 default_provider = $8,
                 updated_at = NOW()"#,
    )
    .bind(p.wallet_payment_enabled)
    .bind(p.wallet_transfer_enabled)
    .bind(p.paypal_enabled)
    .bind(p.stripe_enabled)
    .bind(p.xendit_enabled)
    .bind(p.midtrans_enabled)
    .bind(p.x402_enabled)
    .bind(default_provider.as_deref())
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => {
            info!(
                admin_user_id = %admin_user_id,
                action = "admin_payment_settings_save",
                wallet_payment_enabled = p.wallet_payment_enabled,
                wallet_transfer_enabled = p.wallet_transfer_enabled,
                paypal_enabled = p.paypal_enabled,
                stripe_enabled = p.stripe_enabled,
                xendit_enabled = p.xendit_enabled,
                midtrans_enabled = p.midtrans_enabled,
                x402_enabled = p.x402_enabled,
                default_provider = ?default_provider,
                "payment settings saved"
            );
            Json(json!({"ok": true, "message": "Payment settings saved."}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn admin_smtp_save(
    State(st): State<AdminState>,
    cookies: Cookies,
    Json(p): Json<SmtpSavePayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let existing_password =
        sqlx::query_scalar::<_, String>("SELECT password FROM smtp_settings WHERE id = 1")
            .fetch_optional(&st.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
    let password_to_save = if p.password.is_empty() {
        existing_password
    } else {
        p.password.clone()
    };

    let res = sqlx::query(
        r#"INSERT INTO smtp_settings (id, host, port, username, password, from_email, from_name, use_tls, enabled, updated_at)
           VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, now())
           ON CONFLICT (id) DO UPDATE
             SET host=$1, port=$2, username=$3, password=$4,
                 from_email=$5, from_name=$6, use_tls=$7, enabled=$8, updated_at=now()"#,
    )
    .bind(&p.host)
    .bind(p.port)
    .bind(&p.username)
    .bind(&password_to_save)
    .bind(&p.from_email)
    .bind(&p.from_name)
    .bind(p.use_tls)
    .bind(p.enabled)
    .execute(&st.pool)
    .await;

    if let Err(e) = res {
        return Json(json!({"ok": false, "error": format!("db: {e}")}));
    }
    info!(
        admin_user_id = %admin_user_id,
        action = "admin_smtp_save",
        host = %p.host,
        port = p.port,
        from_email = %p.from_email,
        from_name = %p.from_name,
        use_tls = p.use_tls,
        enabled = p.enabled,
        test_email = ?p.test_email,
        "smtp settings saved"
    );

    // Optional test email
    if let Some(test_to) = &p.test_email {
        if !test_to.trim().is_empty() {
            let cfg = crate::email::SmtpConfig {
                host: p.host.clone(),
                port: p.port as u16,
                username: p.username.clone(),
                password: password_to_save.clone(),
                from_email: p.from_email.clone(),
                from_name: p.from_name.clone(),
                use_tls: p.use_tls,
                enabled: true, // force enabled for test
            };
            if let Err(e) = crate::email::send_test(&cfg, test_to).await {
                return Json(json!({"ok": true, "saved": true, "test_error": e.to_string()}));
            }
            return Json(json!({"ok": true, "saved": true, "test_sent": true}));
        }
    }

    Json(json!({"ok": true, "saved": true}))
}

// ---------------------------------------------------------------------------
// Wallet admin — GET /admin/wallet/transactions?type=&status=&limit=
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WalletTxnQuery {
    pub txn_type: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

pub async fn admin_wallet_transactions(
    State(st): State<AdminState>,
    cookies: Cookies,
    Query(q): Query<WalletTxnQuery>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };
    let limit = q.limit.unwrap_or(100).min(1000);
    let ttype = q.txn_type.as_deref().unwrap_or("");
    let status = q.status.as_deref().unwrap_or("");
    info!(admin_user_id = %admin_user_id, action = "admin_wallet_transactions_view", txn_type = ttype, status = status, limit = limit, "wallet transactions viewed");

    let mut conditions: Vec<String> = vec![];
    if !ttype.is_empty() {
        conditions.push(format!("wt.txn_type = '{}'", ttype.replace('\'', "''")));
    }
    if !status.is_empty() {
        conditions.push(format!("wt.status = '{}'", status.replace('\'', "''")));
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        r#"SELECT wt.id, wt.txn_type, wt.amount_cents, wt.balance_after, wt.status,
                  wt.note, wt.admin_note, wt.created_at::TEXT AS created_at,
                  u.username,
                  u2.username AS ref_username
           FROM wallet_transactions wt
           JOIN users u  ON u.id  = wt.user_id
           LEFT JOIN users u2 ON u2.id = wt.ref_user_id
           {where_clause}
           ORDER BY wt.created_at DESC
           LIMIT {limit}"#
    );

    let rows = sqlx::query(&sql)
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default();

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":            r.try_get::<i64,            _>("id").unwrap_or(0),
                "txn_type":      r.try_get::<String,         _>("txn_type").unwrap_or_default(),
                "amount_cents":  r.try_get::<i64,            _>("amount_cents").unwrap_or(0),
                "balance_after": r.try_get::<i64,            _>("balance_after").unwrap_or(0),
                "status":        r.try_get::<String,         _>("status").unwrap_or_default(),
                "note":          r.try_get::<Option<String>, _>("note").unwrap_or(None),
                "admin_note":    r.try_get::<Option<String>, _>("admin_note").unwrap_or(None),
                "created_at":    r.try_get::<Option<String>, _>("created_at").unwrap_or(None),
                "username":      r.try_get::<String,         _>("username").unwrap_or_default(),
                "ref_username":  r.try_get::<Option<String>, _>("ref_username").unwrap_or(None),
            })
        })
        .collect();

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wallet_transactions")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    let total_pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM wallet_transactions WHERE status='pending'")
            .fetch_one(&st.pool)
            .await
            .unwrap_or(0);

    Json(json!({
        "ok": true,
        "totals": { "all": total, "pending": total_pending },
        "items": items
    }))
}

// ---------------------------------------------------------------------------
// Wallet admin — POST /admin/wallet/transactions/:id/approve
// Approve a deposit → credit user balance. Reject a withdrawal → refund.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WalletActionPayload {
    pub admin_note: Option<String>,
}

pub async fn admin_wallet_approve(
    State(st): State<AdminState>,
    cookies: Cookies,
    Path(txn_id): Path<i64>,
    Json(p): Json<WalletActionPayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let mut tx = match st.pool.begin().await {
        Ok(t) => t,
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let row = sqlx::query(
        "SELECT user_id, txn_type, amount_cents, status FROM wallet_transactions WHERE id = $1 FOR UPDATE"
    )
    .bind(txn_id)
    .fetch_optional(&mut *tx)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": "transaction not found"}));
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": format!("db: {e}")}));
        }
    };

    let status: String = row.try_get("status").unwrap_or_default();
    let txn_type: String = row.try_get("txn_type").unwrap_or_default();
    let user_id: String = row.try_get("user_id").unwrap_or_default();
    let amount_cents: i64 = row.try_get("amount_cents").unwrap_or(0);

    if status != "pending" {
        let _ = tx.rollback().await;
        return Json(
            json!({"ok": false, "error": format!("cannot approve: status is '{status}'")}),
        );
    }
    if txn_type != "deposit" {
        let _ = tx.rollback().await;
        return Json(
            json!({"ok": false, "error": "only deposits can be approved via this endpoint"}),
        );
    }

    let new_bal: i64 = match sqlx::query_scalar(
        "UPDATE users SET balance_cents = balance_cents + $1 WHERE id = $2 RETURNING balance_cents",
    )
    .bind(amount_cents)
    .bind(&user_id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(b) => b,
        Err(e) => {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": format!("db credit: {e}")}));
        }
    };

    if let Err(e) = sqlx::query(
        "UPDATE wallet_transactions SET status='approved', balance_after=$1, admin_note=$2, updated_at=now() WHERE id=$3"
    )
    .bind(new_bal).bind(p.admin_note.as_deref()).bind(txn_id)
    .execute(&mut *tx).await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db update: {e}")}));
    }

    match tx.commit().await {
        Ok(_) => {
            info!(admin_user_id = %admin_user_id, action = "admin_wallet_approve", txn_id = txn_id, user_id = %user_id, txn_type = %txn_type, amount_cents = amount_cents, new_balance_cents = new_bal, admin_note = ?p.admin_note, "wallet deposit approved");
            Json(json!({"ok": true, "new_balance_cents": new_bal}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("commit: {e}")})),
    }
}

// ---------------------------------------------------------------------------
// Wallet admin — POST /admin/wallet/transactions/:id/complete
// ---------------------------------------------------------------------------

pub async fn admin_wallet_complete(
    State(st): State<AdminState>,
    cookies: Cookies,
    Path(txn_id): Path<i64>,
    Json(p): Json<WalletActionPayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let row = sqlx::query("SELECT txn_type, status FROM wallet_transactions WHERE id = $1")
        .bind(txn_id)
        .fetch_optional(&st.pool)
        .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return Json(json!({"ok": false, "error": "transaction not found"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let status: String = row.try_get("status").unwrap_or_default();
    let txn_type: String = row.try_get("txn_type").unwrap_or_default();

    if status != "pending" {
        return Json(json!({"ok": false, "error": format!("status is '{status}', not pending")}));
    }
    if txn_type != "withdrawal" {
        return Json(
            json!({"ok": false, "error": "only withdrawals can be completed via this endpoint"}),
        );
    }

    match sqlx::query(
        "UPDATE wallet_transactions SET status='completed', admin_note=$1, updated_at=now() WHERE id=$2"
    )
    .bind(p.admin_note.as_deref()).bind(txn_id)
    .execute(&st.pool).await
    {
        Ok(_)  => {
            info!(admin_user_id = %admin_user_id, action = "admin_wallet_complete", txn_id = txn_id, txn_type = %txn_type, admin_note = ?p.admin_note, "wallet withdrawal marked completed");
            Json(json!({"ok": true}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

// ---------------------------------------------------------------------------
// Wallet admin — POST /admin/wallet/transactions/:id/reject
// ---------------------------------------------------------------------------

pub async fn admin_wallet_reject(
    State(st): State<AdminState>,
    cookies: Cookies,
    Path(txn_id): Path<i64>,
    Json(p): Json<WalletActionPayload>,
) -> impl IntoResponse {
    let admin_user_id = match ensure_admin_session(&st, &cookies).await {
        Ok(user_id) => user_id,
        Err(resp) => return resp,
    };

    let mut tx = match st.pool.begin().await {
        Ok(t) => t,
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let row = sqlx::query(
        "SELECT user_id, txn_type, amount_cents, status FROM wallet_transactions WHERE id = $1 FOR UPDATE"
    )
    .bind(txn_id)
    .fetch_optional(&mut *tx)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": "transaction not found"}));
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": format!("db: {e}")}));
        }
    };

    let status: String = row.try_get("status").unwrap_or_default();
    let txn_type: String = row.try_get("txn_type").unwrap_or_default();
    let user_id: String = row.try_get("user_id").unwrap_or_default();
    let amount_cents: i64 = row.try_get("amount_cents").unwrap_or(0);

    if status != "pending" {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("status is '{status}', cannot reject")}));
    }

    let is_withdrawal = txn_type == "withdrawal";

    if is_withdrawal {
        if let Err(e) =
            sqlx::query("UPDATE users SET balance_cents = balance_cents + $1 WHERE id = $2")
                .bind(amount_cents)
                .bind(&user_id)
                .execute(&mut *tx)
                .await
        {
            let _ = tx.rollback().await;
            return Json(json!({"ok": false, "error": format!("db refund: {e}")}));
        }
    }

    if let Err(e) = sqlx::query(
        "UPDATE wallet_transactions SET status='rejected', admin_note=$1, updated_at=now() WHERE id=$2"
    )
    .bind(p.admin_note.as_deref()).bind(txn_id)
    .execute(&mut *tx).await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db: {e}")}));
    }

    match tx.commit().await {
        Ok(_) => {
            info!(admin_user_id = %admin_user_id, action = "admin_wallet_reject", txn_id = txn_id, user_id = %user_id, txn_type = %txn_type, amount_cents = amount_cents, refunded = is_withdrawal, admin_note = ?p.admin_note, "wallet transaction rejected");
            Json(json!({"ok": true, "refunded": is_withdrawal}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("commit: {e}")})),
    }
}
