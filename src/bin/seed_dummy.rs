// src/bin/seed_dummy.rs
use anyhow::{anyhow, Context, Result};
use argon2::{Argon2, PasswordHasher};
use argon2::password_hash::{SaltString, rand_core::OsRng};
use chrono::Utc;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok(); // kalau ada .env
    let db_url = std::env::var("DATABASE_URL")
        .context("Missing DATABASE_URL")?;
    let pool = Pool::<Postgres>::connect(&db_url).await
        .context("connect db")?;

    // daftar 10 user dummy
    let mut users = Vec::new();
    for i in 1..=10 {
        let uname = format!("user{:02}", i);
        let email = format!("{}@example.com", uname);
        let password = format!("Passw0rd{:02}!", i);
        users.push((uname, email, password));
    }

    for (username, email, plain) in users {
        // cek eksistensi via email (UNIQUE)
        let exists: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1 FROM users WHERE email = $1 LIMIT 1"#,
        )
        .bind(&email.to_ascii_lowercase())
        .fetch_optional(&pool)
        .await
        .context("query exists")?;

        if exists.is_some() {
            println!("[seed] skip existing {}", email);
            continue;
        }

        // hash argon2 (hindari .context karena error type tidak StdError)
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| anyhow!("argon2 hash: {e}"))?
            .to_string();

        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        sqlx::query!(
            r#"
            INSERT INTO users (id, username, email, password_hash, is_admin, created_at)
            VALUES ($1, $2, $3, $4, 0, $5)
            "#,
            uid,
            username.trim(),
            email.to_ascii_lowercase(),
            hash,
            now
        )
        .execute(&pool)
        .await
        .with_context(|| format!("insert user {}", email))?;

        println!("[seed] inserted {} (user={})", email, username);
    }

    println!("[seed] done.");
    Ok(())
}

