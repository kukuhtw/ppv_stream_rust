// src/db.rs
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use tracing::{info, warn};

pub type PgPool = Pool<Postgres>;

pub async fn new_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./sql").run(&pool).await?;
    apply_extra_migrations(&pool).await?;
    info!("database migrated");
    Ok(pool)
}

/// Menjalankan file SQL di folder `migrations/` (013..035, dst) yang tidak
/// dikelola oleh `sqlx::migrate!("./sql")`. Setiap file dicatat di tabel
/// `_ppv_extra_migrations` sehingga hanya dijalankan sekali. File yang
/// namanya sudah ada di folder `sql/` dilewati (sudah diterapkan oleh sqlx).
///
/// Env:
/// - `EXTRA_MIGRATIONS_DIR` (default `migrations`)
/// - `APPLY_EXTRA_MIGRATIONS` (default `1`; set `0` untuk menonaktifkan)
async fn apply_extra_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let enabled = std::env::var("APPLY_EXTRA_MIGRATIONS")
        .map(|v| !matches!(v.trim(), "0" | "false" | "no"))
        .unwrap_or(true);
    if !enabled {
        return Ok(());
    }

    let dir = std::env::var("EXTRA_MIGRATIONS_DIR").unwrap_or_else(|_| "migrations".into());
    let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "sql").unwrap_or(false))
            .collect(),
        Err(e) => {
            warn!(dir = %dir, error = %e, "extra migrations dir not found, skipped");
            return Ok(());
        }
    };
    files.sort();

    let mut conn = pool.acquire().await?;
    // Lock agar app + watcher tidak menjalankan migrasi bersamaan.
    sqlx::raw_sql("SELECT pg_advisory_lock(7240402)")
        .execute(&mut *conn)
        .await?;

    let result = async {
        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS _ppv_extra_migrations (
                filename   TEXT PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(&mut *conn)
        .await?;

        for path in files {
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if std::path::Path::new("sql").join(&name).exists() {
                continue; // sudah ditangani sqlx::migrate!("./sql")
            }
            let done: Option<(String,)> =
                sqlx::query_as("SELECT filename FROM _ppv_extra_migrations WHERE filename = $1")
                    .bind(&name)
                    .fetch_optional(&mut *conn)
                    .await?;
            if done.is_some() {
                continue;
            }

            let body = std::fs::read_to_string(&path)?;
            info!(file = %name, "applying extra migration");
            sqlx::raw_sql(&body)
                .execute(&mut *conn)
                .await
                .map_err(|e| anyhow::anyhow!("extra migration {name} failed: {e}"))?;
            sqlx::query("INSERT INTO _ppv_extra_migrations (filename) VALUES ($1)")
                .bind(&name)
                .execute(&mut *conn)
                .await?;
        }
        anyhow::Ok(())
    }
    .await;

    let _ = sqlx::raw_sql("SELECT pg_advisory_unlock(7240402)")
        .execute(&mut *conn)
        .await;
    result
}
