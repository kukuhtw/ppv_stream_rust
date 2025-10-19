// src/auth.rs

use axum::{extract::State, response::{IntoResponse, Redirect}, Form};
use serde::Deserialize;
use tower_cookies::{Cookies, Cookie};
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthState { pub pool: SqlitePool }

#[derive(Deserialize)]
pub struct LoginForm { pub username: String }

/// Demo login by username (legacy). Prefer `/auth/login` with email+password.
pub async fn post_login(State(st): State<AuthState>, cookies: Cookies, Form(f): Form<LoginForm>) -> impl IntoResponse {
    let username = f.username.trim();
    if username.is_empty() { return Redirect::to("/dashboard"); }
    let now = Utc::now().to_rfc3339();
    // try insert user if not exists
    let existing = sqlx::query!("SELECT id FROM users WHERE username=$1", username)
        .fetch_optional(&st.pool).await.unwrap();
    let user_id = if let Some(r) = existing { r.id.unwrap() } else {
        let id = Uuid::new_v4().to_string();
        let _ = sqlx::query!("INSERT INTO users (id, username, created_at) VALUES ($1,$2,$3)",
            id, username, now).execute(&st.pool).await;
        id
    };
    cookies.add(Cookie::new("username", username.to_string()));
    cookies.add(Cookie::new("user_id", user_id));
    Redirect::to("/dashboard")
}

pub fn current_username(cookies: &Cookies) -> Option<String> {
    cookies.get("username").map(|c| c.value().to_string())
}
