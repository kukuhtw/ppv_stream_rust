// src/handlers/admin.rs
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;

#[derive(Clone)]
pub struct AdminState {
    pub pool: crate::db::PgPool,
}

// (asumsikan kamu sudah punya handler admin_dashboard & admin_stats di file ini)

#[derive(Deserialize)]
pub struct AdminListQuery {
    pub limit: Option<usize>,
}

pub async fn admin_data(
    State(st): State<AdminState>,
    // Jika ingin cek admin session, aktifkan cookies + cek session:
    // cookies: Cookies,
    Query(q): Query<AdminListQuery>,
) -> Html<String> {
    // ---- (Opsional) Cek admin dari session cookie ----
    // match sessions::current_user_id(&st.pool, &cookies).await {
    //     Ok(Some((_uid, is_admin))) if is_admin => {},
    //     _ => return Html("<h1>Forbidden</h1><p>Admin only</p>".into()),
    // }

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
        r#"SELECT id, username, email, is_admin, created_at
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
        r#"SELECT id, user_id, token, expires_at, used, created_at
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
    html.push_str(&format!(r#"
<div class="card"><h2>users <span class="caps">(total: {})</span></h2>
<table>
  <thead><tr><th>id</th><th>username</th><th>email</th><th>is_admin</th><th>created_at</th></tr></thead>
  <tbody>
"#, total_users));

    for r in &users {
        let id: String = r.get("id");
        let username: String = r.get("username");
        let email: Option<String> = r.try_get("email").ok();
        let is_admin: i32 = r.get("is_admin");
        let created_at: String = r.get("created_at");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='mono'>{}</td></tr>",
            esc(&id), esc(&username), esc(email.as_deref().unwrap_or("")),
            is_admin, esc(&created_at)
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
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td class='mono'>{}</td><td>{}</td><td class='mono'>{}</td><td class='mono'>{}</td></tr>",
            esc(&id), esc(user_id.as_deref().unwrap_or("")),
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
        let token: String = r.get("token");
        let expires_at: String = r.get("expires_at");
        let used: i32 = r.get("used");
        let created_at: String = r.get("created_at");
        html.push_str(&format!(
            "<tr><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td><td class='mono'>{}</td><td>{}</td><td class='mono'>{}</td></tr>",
            id, esc(&user_id), esc(&token), esc(&expires_at), used, esc(&created_at)
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
    pub status:   Option<String>,
    pub limit:    Option<i64>,
}

pub async fn admin_payments(
    State(st): State<AdminState>,
    Query(q):  Query<PaymentsQuery>,
) -> impl IntoResponse {
    let limit    = q.limit.unwrap_or(100).min(1000);
    let provider = q.provider.as_deref().unwrap_or("");
    let status   = q.status.as_deref().unwrap_or("");

    // Build WHERE clauses dynamically using runtime query (not macro) to avoid DB-at-build-time
    let base_sql = r#"
        SELECT
            fi.invoice_uid,
            fi.provider,
            fi.status,
            fi.amount,
            fi.currency,
            fi.payment_url,
            fi.buyer_email,
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
    if !provider.is_empty() { conditions.push(format!("fi.provider = '{}'", provider.replace('\'', "''"))); }
    if !status.is_empty()   { conditions.push(format!("fi.status = '{}'",   status.replace('\'', "''"))); }

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

    let items: Vec<serde_json::Value> = rows.iter().map(|r| {
        json!({
            "invoice_uid":      r.try_get::<String, _>("invoice_uid").unwrap_or_default(),
            "provider":         r.try_get::<String, _>("provider").unwrap_or_default(),
            "status":           r.try_get::<String, _>("status").unwrap_or_default(),
            "amount":           r.try_get::<i64,   _>("amount").unwrap_or(0),
            "currency":         r.try_get::<String, _>("currency").unwrap_or_default(),
            "payment_url":      r.try_get::<Option<String>, _>("payment_url").unwrap_or(None),
            "buyer_email":      r.try_get::<Option<String>, _>("buyer_email").unwrap_or(None),
            "created_at":       r.try_get::<Option<String>, _>("created_at").unwrap_or(None),
            "paid_at":          r.try_get::<Option<String>, _>("paid_at").unwrap_or(None),
            "disbursed_at":     r.try_get::<Option<String>, _>("disbursed_at").unwrap_or(None),
            "disburse_ref":     r.try_get::<Option<String>, _>("disburse_ref").unwrap_or(None),
            "buyer_username":   r.try_get::<String, _>("buyer_username").unwrap_or_default(),
            "video_title":      r.try_get::<Option<String>, _>("video_title").unwrap_or(None),
            "creator_username": r.try_get::<String, _>("creator_username").unwrap_or_default(),
            "creator_bank":     r.try_get::<Option<String>, _>("creator_bank").unwrap_or(None),
        })
    }).collect();

    // Totals for filter status badge counts
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices")
        .fetch_one(&st.pool).await.unwrap_or(0);
    let total_paid: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices WHERE status='paid'")
        .fetch_one(&st.pool).await.unwrap_or(0);
    let total_pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices WHERE status='pending'")
        .fetch_one(&st.pool).await.unwrap_or(0);
    let total_failed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fiat_invoices WHERE status IN ('failed','expired','cancelled')")
        .fetch_one(&st.pool).await.unwrap_or(0);

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
    State(st):       State<AdminState>,
    Path(uid):       Path<String>,
) -> impl IntoResponse {
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
        Ok(None)    => return Json(json!({"ok": false, "error": "invoice not found"})),
        Err(e)      => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let status:       String         = row.try_get("status").unwrap_or_default();
    let disbursed_at: Option<String> = row.try_get("disbursed_at").unwrap_or(None);
    let provider:     String         = row.try_get("provider").unwrap_or_default();
    let amount:       i64            = row.try_get("amount").unwrap_or(0);

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
                        return Json(json!({"ok": true, "disburse_ref": disburse_ref, "method": "xendit_api"}));
                    }
                    Err(e) => {
                        return Json(json!({"ok": false, "error": format!("xendit disburse: {e}")}));
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

    Json(json!({"ok": true, "method": "manual", "invoice_uid": uid}))
}

// ---------------------------------------------------------------------------
// SMTP settings — GET /admin/smtp  /  POST /admin/smtp
// ---------------------------------------------------------------------------

pub async fn admin_smtp_get(State(st): State<AdminState>) -> impl IntoResponse {
    let row = sqlx::query(
        "SELECT host, port, username, password, from_email, from_name, use_tls, enabled
         FROM smtp_settings WHERE id = 1"
    )
    .fetch_optional(&st.pool)
    .await;

    match row {
        Ok(Some(r)) => {
            Json(json!({
                "ok": true,
                "smtp": {
                    "host":       r.try_get::<String,  _>("host").unwrap_or_default(),
                    "port":       r.try_get::<i32,     _>("port").unwrap_or(587),
                    "username":   r.try_get::<String,  _>("username").unwrap_or_default(),
                    "password":   r.try_get::<String,  _>("password").unwrap_or_default(),
                    "from_email": r.try_get::<String,  _>("from_email").unwrap_or_default(),
                    "from_name":  r.try_get::<String,  _>("from_name").unwrap_or_else(|_| "PPV Stream".into()),
                    "use_tls":    r.try_get::<bool,    _>("use_tls").unwrap_or(true),
                    "enabled":    r.try_get::<bool,    _>("enabled").unwrap_or(false),
                }
            }))
        }
        _ => Json(json!({"ok": false, "error": "smtp_settings not found"})),
    }
}

#[derive(Deserialize)]
pub struct SmtpSavePayload {
    pub host:       String,
    pub port:       i32,
    pub username:   String,
    pub password:   String,
    pub from_email: String,
    pub from_name:  String,
    pub use_tls:    bool,
    pub enabled:    bool,
    /// Optional: send a test email to this address after saving
    pub test_email: Option<String>,
}

pub async fn admin_smtp_save(
    State(st): State<AdminState>,
    Json(p):   Json<SmtpSavePayload>,
) -> impl IntoResponse {
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
    .bind(&p.password)
    .bind(&p.from_email)
    .bind(&p.from_name)
    .bind(p.use_tls)
    .bind(p.enabled)
    .execute(&st.pool)
    .await;

    if let Err(e) = res {
        return Json(json!({"ok": false, "error": format!("db: {e}")}));
    }

    // Optional test email
    if let Some(test_to) = &p.test_email {
        if !test_to.trim().is_empty() {
            let cfg = crate::email::SmtpConfig {
                host:       p.host.clone(),
                port:       p.port as u16,
                username:   p.username.clone(),
                password:   p.password.clone(),
                from_email: p.from_email.clone(),
                from_name:  p.from_name.clone(),
                use_tls:    p.use_tls,
                enabled:    true, // force enabled for test
            };
            if let Err(e) = crate::email::send_test(&cfg, test_to).await {
                return Json(json!({"ok": true, "saved": true, "test_error": e.to_string()}));
            }
            return Json(json!({"ok": true, "saved": true, "test_sent": true}));
        }
    }

    Json(json!({"ok": true, "saved": true}))
}
