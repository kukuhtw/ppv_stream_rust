// src/bootstrap.rs
use sqlx::{SqlitePool, Acquire};
use chrono::Utc;
use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use tracing::{info, warn};

pub async fn ensure_admin_from_env(
    pool: &SqlitePool,
    email: Option<String>,
    password: Option<String>,
) -> sqlx::Result<()> {
    // 1) Cek cepat: sudah ada admin?
    let exists: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) as 'count!: i64' FROM users WHERE is_admin=1"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if exists > 0 {
        info!("Admin already exists, skip env bootstrap");
        return Ok(());
    }

    // 2) Ambil kredensial ENV
    let (Some(email_raw), Some(password)) = (email, password) else {
        warn!("No admin exists yet, but ADMIN_BOOTSTRAP_EMAIL/PASSWORD not set. You can use /admin/setup with token.");
        return Ok(());
    };
    let email = email_raw.trim().to_ascii_lowercase();

    // 3) Siapkan hash
    let now = Utc::now().to_rfc3339();
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string();

    // 4) Transaksi + cek lagi untuk race-safety
    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;

    let exists_tx: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) as 'count!: i64' FROM users WHERE is_admin=1"
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0);

    if exists_tx > 0 {
        info!("Admin already exists (checked in tx), skip env bootstrap");
        tx.commit().await?;
        return Ok(());
    }

    // Pastikan unique index email (no-op kalau sudah ada)
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS users_email_uq ON users(email)")
        .execute(&mut *tx)
        .await
        .ok();

    // Upsert by email → admin
    let id = uuid::Uuid::new_v4().to_string();
    let username = "admin";

    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, is_admin, created_at)
        VALUES ($1,$2,$3,$4,1,$5)
        ON CONFLICT(email) DO UPDATE SET
            is_admin=1,
            password_hash=excluded.password_hash
        "#
    )
    .bind(&id)
    .bind(username)
    .bind(&email)
    .bind(&hash)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    info!("ensure_admin_from_env: ensured admin account via env");
    Ok(())
}
