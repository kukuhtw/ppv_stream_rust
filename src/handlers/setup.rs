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
use tracing::{info, warn};

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
    // This endpoint is intentionally single-purpose and temporary: it exists
    // only to bootstrap the first admin account when an explicit token is
    // configured at deploy time.
    let Some(required) = &st.token else {
        warn!(
            action = "setup_admin_blocked",
            reason = "bootstrap_disabled",
            "admin bootstrap attempted while disabled"
        );
        return Html("<h1>Unavailable</h1><p>Admin bootstrap is disabled.</p>".into());
    };
    if q.token.as_deref() != Some(required.as_str()) {
        warn!(
            action = "setup_admin_blocked",
            reason = "invalid_token",
            "admin bootstrap attempted with invalid token"
        );
        return Html("<h1>Unauthorized</h1><p>Invalid or missing token.</p>".into());
    }

    // Once any admin exists, bootstrap must permanently shut itself off to
    // avoid becoming an alternate account-recovery path.
    let admin_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE is_admin = 1")
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    if admin_count > 0 {
        warn!(
            action = "setup_admin_blocked",
            reason = "admin_exists",
            "admin bootstrap attempted after admin already exists"
        );
        return Html("<h1>Forbidden</h1><p>Admin bootstrap is no longer available after an admin account exists.</p>".into());
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

    // If the bootstrap email already belongs to an existing account, promote
    // that account instead of silently creating a duplicate identity.
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
            info!(action = "setup_admin_success", mode = "promote_existing", email = %email_norm, "admin bootstrap succeeded");
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
            info!(action = "setup_admin_success", mode = "create_new", email = %email_norm, "admin bootstrap succeeded");
            Html("<h1>OK</h1><p>Admin user created.</p>".into())
        }
        Err(e) => Html(format!("<h1>Error</h1><pre>SELECT failed: {e}</pre>")),
    }
}
