// src/handlers/auth_admin.rs

use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tower_cookies::Cookies;

use argon2::password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use sqlx::{PgPool, Row};

use crate::sessions;
use crate::config::Config;

#[derive(Clone)]
pub struct AuthAdminState {
    pub pool: PgPool,
    pub cfg:  Config, // <-- penting: untuk create_session/destroy_session
}

#[derive(Deserialize)]
pub struct AdminLoginForm {
    pub email: String,
    pub password: String,
}

pub async fn post_admin_login(
    State(st): State<AuthAdminState>,
    cookies: Cookies,
    Form(f): Form<AdminLoginForm>,
) -> impl IntoResponse {
    if f.email.trim().is_empty() || f.password.trim().is_empty() {
        return Redirect::to("/public/admin/login.html?status=fail&reason=missing_field");
    }

    // Ambil user admin
    let row = match sqlx::query!(
        r#"SELECT id, password_hash, is_admin FROM users WHERE email=$1 LIMIT 1"#,
        f.email.to_ascii_lowercase()
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(_) => {
            return Redirect::to("/public/admin/login.html?status=fail&reason=server_error")
        }
    };

    let Some(r) = row else {
        return Redirect::to("/public/admin/login.html?status=fail&reason=bad_credentials");
    };

    // Wajib admin
    if r.is_admin == 0 {
        return Redirect::to("/public/admin/login.html?status=fail&reason=not_admin");
    }

    // password_hash nullable → amankan
    let Some(ph) = r.password_hash.as_deref() else {
        return Redirect::to("/public/admin/login.html?status=fail&reason=bad_credentials");
    };

    let Ok(parsed) = PasswordHash::new(ph) else {
        return Redirect::to("/public/admin/login.html?status=fail&reason=server_error");
    };

    if Argon2::default()
        .verify_password(f.password.as_bytes(), &parsed)
        .is_ok()
    {
        let uid: &str = &r.id;
        if sessions::create_session(&st.pool, &st.cfg, uid, true, &cookies)
            .await
            .is_err()
        {
            return Redirect::to("/public/admin/login.html?status=fail&reason=server_error");
        }
        // ✅ SUKSES → ke dashboard admin
        Redirect::to("/public/admin/dashboard.html?status=ok")
    } else {
        Redirect::to("/public/admin/login.html?status=fail&reason=bad_credentials")
    }
}

pub async fn post_admin_logout(
    State(st): State<AuthAdminState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let _ = sessions::destroy_session(&st.pool, &st.cfg, &cookies).await;
    Redirect::to("/public/admin/login.html?status=ok")
}

// ---------------------------------------------------------------------------
// POST /admin/change_password  (requires active admin session)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AdminChangePasswordPayload {
    pub current_password: String,
    pub new_password:     String,
}

pub async fn admin_change_password(
    State(st):  State<AuthAdminState>,
    cookies:    Cookies,
    Json(payload): Json<AdminChangePasswordPayload>,
) -> impl IntoResponse {
    // Require admin session
    let Some((user_id, is_admin)) = sessions::current_user_id(&st.pool, &st.cfg, &cookies).await else {
        return Json(json!({"ok": false, "error": "not logged in"}));
    };
    if !is_admin {
        return Json(json!({"ok": false, "error": "not admin"}));
    }

    if payload.new_password.len() < 8 {
        return Json(json!({"ok": false, "error": "Password baru terlalu pendek (min 8 karakter)"}));
    }
    if payload.current_password == payload.new_password {
        return Json(json!({"ok": false, "error": "Password baru harus berbeda dari password lama"}));
    }

    // Fetch current hash, email, username
    let row = sqlx::query("SELECT password_hash, email, username FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_optional(&st.pool)
        .await;

    let row = match row {
        Ok(Some(r)) => r,
        _ => return Json(json!({"ok": false, "error": "user not found"})),
    };

    let ph_opt: Option<String> = row.try_get("password_hash").ok();
    let Some(ph) = ph_opt.as_deref() else {
        return Json(json!({"ok": false, "error": "server error: no password hash"}));
    };

    let parsed = match PasswordHash::new(ph) {
        Ok(h) => h,
        Err(_) => return Json(json!({"ok": false, "error": "server error"})),
    };
    if Argon2::default()
        .verify_password(payload.current_password.as_bytes(), &parsed)
        .is_err()
    {
        return Json(json!({"ok": false, "error": "Password lama tidak sesuai"}));
    }

    let salt = SaltString::generate(&mut OsRng);
    let new_hash = match Argon2::default().hash_password(payload.new_password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => return Json(json!({"ok": false, "error": "server error"})),
    };

    if sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(&user_id)
        .execute(&st.pool)
        .await
        .is_err()
    {
        return Json(json!({"ok": false, "error": "db error"}));
    }

    // Send notification email (fire-and-forget)
    let pool_clone = st.pool.clone();
    let email_addr: String = row.try_get::<Option<String>, _>("email").ok().flatten().unwrap_or_default();
    let username: String   = row.try_get("username").unwrap_or_default();
    tokio::spawn(async move {
        crate::email::send_password_changed(&pool_clone, &email_addr, &username).await;
    });

    Json(json!({"ok": true}))
}
