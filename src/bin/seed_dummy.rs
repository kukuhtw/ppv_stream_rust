// src/bin/seed_dummy.rs
#![allow(clippy::needless_return)]

use anyhow::{anyhow, Context, Result};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use chrono::Utc;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    // Kalau .env kamu ada baris "aneh", sementara disable saja:
    // dotenvy::dotenv().ok();

    // Lebih aman: baca langsung dari env process (export di shell/Makefile)
    let db_url = std::env::var("DATABASE_URL").context("Missing DATABASE_URL")?;
    let pool = Pool::<Postgres>::connect(&db_url)
        .await
        .context("connect db")?;

    // Siapkan 10 user dummy
    let mut users = Vec::new();
    for i in 1..=10 {
        let uname = format!("user{:02}", i);
        let email = format!("{}@example.com", uname);
        let password = format!("Passw0rd{:02}!", i);
        users.push((uname, email, password));
    }

    for (username, email, plain) in users {
        let email_lc = email.to_ascii_lowercase();

        // Cek exist dengan query runtime (ini sudah runtime, bukan macro !)
        let exists: Option<i32> = sqlx::query_scalar::<_, i32>(
            r#"SELECT 1 FROM users WHERE email = $1 LIMIT 1"#,
        )
        .bind(&email_lc)
        .fetch_optional(&pool)
        .await
        .context("query exists")?;

        if exists.is_some() {
            println!("[seed] skip existing {}", email_lc);
            continue;
        }

        // Hash Argon2
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| anyhow!("argon2 hash: {e}"))?
            .to_string();

        // UUID sebagai TEXT
        let uid = Uuid::new_v4().to_string();

        // Timestamp RFC3339 untuk kolom TEXT created_at
        let now_rfc3339 = Utc::now().to_rfc3339();

        // Insert pakai sqlx::query (BUKAN query!)
        let rows = sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, is_admin, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (email) DO NOTHING
            "#,
        )
        .bind(&uid)               // $1
        .bind(username.trim())    // $2
        .bind(&email_lc)          // $3
        .bind(&hash)              // $4
        .bind(0i32)               // $5
        .bind(&now_rfc3339)       // $6
        .execute(&pool)
        .await
        .with_context(|| format!("insert user {}", username))?;

        if rows.rows_affected() > 0 {
            println!("[seed] inserted {}", username);
        } else {
            println!("[seed] skipped (conflict) {}", username);
        }
    }

    println!("[seed] done.");
    Ok(())
}
