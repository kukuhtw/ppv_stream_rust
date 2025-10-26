// src/handlers/auth_user.rs
// src/handlers/auth_user.rs

use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use tower_cookies::Cookies;

use argon2::password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};

use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{email, sessions, validators};
use crate::config::Config;

#[derive(Clone)]
pub struct AuthUserState {
    pub pool: PgPool,
    pub cfg:  Config,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub email: String,
    pub password: String,
}

pub async fn post_register(
    State(st): State<AuthUserState>,
    Form(f): Form<RegisterForm>,
) -> impl IntoResponse {
    // Validasi sederhana
    if f.username.trim().is_empty() || f.email.trim().is_empty() || f.password.trim().is_empty() {
        return Redirect::to("/public/auth/register.html?status=fail&reason=missing_field");
    }
    if !validators::valid_email(&f.email) {
        return Redirect::to("/public/auth/register.html?status=fail&reason=invalid_email");
    }
    if !validators::valid_password(&f.password) {
        return Redirect::to("/public/auth/register.html?status=fail&reason=weak_password");
    }

    // Cek email sudah terpakai
    if let Ok(Some(_)) =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM users WHERE email = $1 LIMIT 1")
            .bind(f.email.to_ascii_lowercase())
            .fetch_optional(&st.pool)
            .await
    {
        return Redirect::to("/public/auth/register.html?status=fail&reason=email_taken");
    }

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let hash = match Argon2::default().hash_password(f.password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => {
            return Redirect::to("/public/auth/register.html?status=fail&reason=server_error")
        }
    };

    let uid = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Simpan user (is_admin = 0)
    let res = sqlx::query!(
        r#"
        INSERT INTO users (id, username, email, password_hash, is_admin, created_at)
        VALUES ($1, $2, $3, $4, 0, $5)
        "#,
        uid,
        f.username.trim(),
        f.email.to_ascii_lowercase(),
        hash,
        now
    )
    .execute(&st.pool)
    .await;

    if res.is_err() {
        // Bisa jadi race condition email unique
        return Redirect::to("/public/auth/register.html?status=fail&reason=server_error");
    }

    // Sukses → arahkan ke login dengan status ok
    Redirect::to("/public/auth/login.html?status=ok")
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

pub async fn post_login(
    State(st): State<AuthUserState>,
    cookies: Cookies,
    Form(f): Form<LoginForm>,
) -> impl IntoResponse {
    if f.email.trim().is_empty() || f.password.trim().is_empty() {
        return Redirect::to("/public/auth/login.html?status=fail&reason=missing_field");
    }
    if !validators::valid_email(&f.email) {
        return Redirect::to("/public/auth/login.html?status=fail&reason=invalid_email");
    }

    // Ambil user + password_hash
    let row = match sqlx::query!(
        r#"SELECT id, password_hash FROM users WHERE email=$1 LIMIT 1"#,
        f.email.to_ascii_lowercase()
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(_) => return Redirect::to("/public/auth/login.html?status=fail&reason=server_error"),
    };

    // Tidak ada user
    let Some(row) = row else {
        // Jangan bocorkan "user_not_found" di UI publik; tetap generik
        return Redirect::to("/public/auth/login.html?status=fail&reason=bad_credentials");
    };

    // Kolom password_hash mungkin nullable
    let Some(ph) = row.password_hash.as_deref() else {
        return Redirect::to("/public/auth/login.html?status=fail&reason=bad_credentials");
    };

    // Parse hash Argon2
    let Ok(parsed) = PasswordHash::new(ph) else {
        return Redirect::to("/public/auth/login.html?status=fail&reason=server_error");
    };

    // Verifikasi
    if Argon2::default()
        .verify_password(f.password.as_bytes(), &parsed)
        .is_ok()
    {
        let uid: &str = &row.id;
        if sessions::create_session(&st.pool, &st.cfg, uid, false, &cookies)
            .await
            .is_err()
        {
            return Redirect::to("/public/auth/login.html?status=fail&reason=server_error");
        }
        // ✅ SUKSES → ke dashboard user
        Redirect::to("/public/dashboard.html?status=ok")
    } else {
        Redirect::to("/public/auth/login.html?status=fail&reason=bad_credentials")
    }
}

pub async fn post_logout(State(st): State<AuthUserState>, cookies: Cookies) -> impl IntoResponse {
    let _ = sessions::destroy_session(&st.pool, &st.cfg, &cookies).await;
    Redirect::to("/public/auth/login.html?status=ok")
}

#[derive(Deserialize)]
pub struct ForgotForm {
    pub email: String,
}

pub async fn post_forgot(
    State(st): State<AuthUserState>,
    Form(f): Form<ForgotForm>,
) -> impl IntoResponse {
    if !validators::valid_email(&f.email) {
        return Redirect::to("/public/auth/forgot_password.html?status=fail&reason=invalid_email");
    }

    // Selalu redirect ok (jangan bocorkan apakah email ada/tidak)
    // Namun tetap proses bila ada usernya
    match sqlx::query!(
        r#"SELECT id FROM users WHERE email=$1 LIMIT 1"#,
        f.email.to_ascii_lowercase()
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(Some(u)) => {
            let uid: &str = &u.id;
            let token = Uuid::new_v4().to_string();
            let now = Utc::now();
            let exp = now + Duration::hours(2);

            let _ = sqlx::query!(
                r#"
                INSERT INTO password_resets (user_id, token, expires_at, used, created_at)
                VALUES ($1, $2, $3, 0, $4)
                "#,
                uid,
                token,
                exp.to_rfc3339(),
                now.to_rfc3339()
            )
            .execute(&st.pool)
            .await;

            let _ = email::send_reset(&f.email, &token).await;
        }
        _ => { /* abaikan error & none untuk tidak bocorkan info */ }
    }

    Redirect::to("/public/auth/login.html?status=ok")
}

#[derive(Deserialize)]
pub struct ResetForm {
    pub token: String,
    pub password: String,
}

pub async fn post_reset(
    State(st): State<AuthUserState>,
    Form(f): Form<ResetForm>,
) -> impl IntoResponse {
    if !validators::valid_password(&f.password) {
        return Redirect::to("/public/auth/reset_password.html?status=fail&reason=weak_password");
    }

    // Ambil token
    let row = match sqlx::query!(
        r#"SELECT user_id, expires_at, used FROM password_resets WHERE token=$1 LIMIT 1"#,
        f.token
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(_) => {
            return Redirect::to("/public/auth/reset_password.html?status=fail&reason=server_error")
        }
    };

    let Some(r) = row else {
        return Redirect::to("/public/auth/reset_password.html?status=fail&reason=invalid_token");
    };

    // used: 0/1
    if r.used != 0 {
        return Redirect::to("/public/auth/reset_password.html?status=fail&reason=token_used");
    }

    // expires_at: TEXT NOT NULL → String; parse langsung
    let exp_dt = match chrono::DateTime::parse_from_rfc3339(&r.expires_at)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
    {
        Some(dt) => dt,
        None => {
            return Redirect::to(
                "/public/auth/reset_password.html?status=fail&reason=invalid_token",
            )
        }
    };

    if exp_dt < Utc::now() {
        return Redirect::to("/public/auth/reset_password.html?status=fail&reason=token_expired");
    }

    // Hash password baru
    let salt = SaltString::generate(&mut OsRng);
    let hash = match Argon2::default().hash_password(f.password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => {
            return Redirect::to("/public/auth/reset_password.html?status=fail&reason=server_error")
        }
    };

    let uid: &str = &r.user_id;

    // Update user
    if sqlx::query!(
        r#"UPDATE users SET password_hash=$1 WHERE id=$2"#,
        hash,
        uid
    )
    .execute(&st.pool)
    .await
    .is_err()
    {
        return Redirect::to("/public/auth/reset_password.html?status=fail&reason=server_error");
    }

    // Tandai token used
    let _ = sqlx::query!(
        r#"UPDATE password_resets SET used=1 WHERE token=$1"#,
        f.token
    )
    .execute(&st.pool)
    .await;

    // Sukses → balik ke login
    Redirect::to("/public/auth/login.html?status=ok")
}
