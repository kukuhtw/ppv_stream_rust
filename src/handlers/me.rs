// src/handlers/me.rs
// src/handlers/me.rs
use axum::{extract::State, response::IntoResponse, Json};
use sqlx::PgPool;
use tower_cookies::Cookies;

use crate::config::Config;
use crate::sessions;

#[derive(Clone)]
pub struct MeState {
    pub pool: PgPool,
    pub cfg:  Config,
}

pub async fn me(State(st): State<MeState>, cookies: Cookies) -> impl IntoResponse {
    let Some((uid, _is_admin)) = sessions::current_user_id(&st.pool, &st.cfg, &cookies).await else {
        return Json(serde_json::json!({"ok": false, "error": "not logged in"}));
    };

    let row = match sqlx::query!(
        r#"SELECT id, username, email FROM users WHERE id = $1 LIMIT 1"#,
        uid
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    if let Some(u) = row {
        Json(serde_json::json!({"ok": true, "user": {
            "id": u.id, "username": u.username, "email": u.email
        }}))
    } else {
        Json(serde_json::json!({"ok": false, "error": "user not found"}))
    }
}
