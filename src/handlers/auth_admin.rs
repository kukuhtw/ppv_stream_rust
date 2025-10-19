
// src/handlers/auth_admin.rs
// src/handlers/auth_admin.rs

use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use tower_cookies::Cookies;

use argon2::{Argon2, PasswordVerifier};
use argon2::password_hash::PasswordHash;
use sqlx::PgPool;

use crate::sessions;

#[derive(Clone)]
pub struct AuthAdminState {
    pub pool: PgPool,
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
        Err(_) => return Redirect::to("/public/admin/login.html?status=fail&reason=server_error"),
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
        if sessions::create_session(&st.pool, uid, true, &cookies).await.is_err() {
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
    let _ = sessions::destroy_session(&st.pool, &cookies).await;
    Redirect::to("/public/admin/login.html?status=ok")
}
