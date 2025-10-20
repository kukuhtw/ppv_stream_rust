// src/handlers/admin.rs
use axum::{
    extract::{Query, State},
    response::Html,
};
use serde::Deserialize;
use sqlx::Row; // untuk akses row.get::<T,_>()
               // use crate::sessions; // jika ingin cek session admin
               // use tower_cookies::Cookies;

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
