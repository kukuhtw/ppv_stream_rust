// src/db.rs
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use tracing::info;

pub type PgPool = Pool<Postgres>;

pub async fn new_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./sql").run(&pool).await?;
    info!("database migrated");
    Ok(pool)
}
