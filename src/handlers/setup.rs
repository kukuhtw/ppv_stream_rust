// src/handlers/setup.rs
use axum::{
    extract::{Query, State},
    response::Html,
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};

#[derive(Clone)]
pub struct SetupState {
    pub pool: PgPool,
    pub token: Option<String>, // token dari ENV (opsional)
}

#[derive(Deserialize)]
pub struct SetupQuery {
    pub token: Option<String>, // <-- opsional agar tidak error kalau tidak dikirim
}

pub async fn setup_admin(
    State(st): State<SetupState>,
    Query(q): Query<SetupQuery>,
) -> Html<String> {
    // Jika ENV token diset, wajib cocok dengan query ?token=...
    if let Some(required) = &st.token {
        if q.token.as_deref() != Some(required.as_str()) {
            return Html("<h1>Unauthorized</h1><p>Invalid or missing token.</p>".into());
        }
    }

    // Baca email & password bootstrap dari ENV
    let email =
        std::env::var("ADMIN_BOOTSTRAP_EMAIL").unwrap_or_else(|_| "admin@example.com".into());
    let password =
        std::env::var("ADMIN_BOOTSTRAP_PASSWORD").unwrap_or_else(|_| "ChangeMe123!".into());

    let email_norm = email.to_ascii_lowercase();

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let hash = match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => return Html("<h1>Error</h1><p>Failed to hash password.</p>".into()),
    };

    // Sudah ada user dengan email tsb?
    match sqlx::query!(r#"SELECT id FROM users WHERE email=$1"#, email_norm)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(Some(u)) => {
            // Promote jadi admin + reset password
            let uid: &str = &u.id;
            if let Err(e) = sqlx::query!(
                r#"UPDATE users SET is_admin=1, password_hash=$1 WHERE id=$2"#,
                hash,
                uid
            )
            .execute(&st.pool)
            .await
            {
                return Html(format!("<h1>Error</h1><pre>UPDATE failed: {e}</pre>"));
            }
            Html("<h1>OK</h1><p>Existing user promoted to admin and password reset.</p>".into())
        }
        Ok(None) => {
            // Buat user admin baru
            let uid = Uuid::new_v4().to_string();
            let username = email.split('@').next().unwrap_or("Admin");
            let now = Utc::now().to_rfc3339();
            if let Err(e) = sqlx::query!(
                r#"
                INSERT INTO users (id, username, email, password_hash, is_admin, created_at)
                VALUES ($1,$2,$3,$4,1,$5)
                "#,
                uid,
                username,
                email,
                hash,
                now
            )
            .execute(&st.pool)
            .await
            {
                return Html(format!("<h1>Error</h1><pre>INSERT failed: {e}</pre>"));
            }
            Html("<h1>OK</h1><p>Admin user created.</p>".into())
        }
        Err(e) => Html(format!("<h1>Error</h1><pre>SELECT failed: {e}</pre>")),
    }
}
