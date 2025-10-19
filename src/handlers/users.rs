

// src/handlers/users.rs

use axum::{extract::State, Json};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct UsersState { pub pool: SqlitePool }

pub async fn list_users(State(st): State<UsersState>) -> Json<serde_json::Value> {
    let rows = sqlx::query!("SELECT id, username, email, is_admin, created_at FROM users ORDER BY created_at DESC")
        .fetch_all(&st.pool).await.unwrap_or_default();
    Json(serde_json::json!({"users": rows}))
}
