use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tower_cookies::Cookies;

use crate::config::Config;
use crate::sessions;

#[derive(Clone)]
pub struct CreatorBlockState {
    pub pool: PgPool,
    pub cfg: Config,
}

#[derive(Deserialize)]
pub struct BlockUserPayload {
    pub user_id: String,
    pub ban_type: Option<String>,
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct UnblockUserPayload {
    pub user_id: String,
}

#[derive(Serialize)]
struct BlockedUserItem {
    blocked_user_id: String,
    username: String,
    email: String,
    ban_type: String,
    reason: Option<String>,
    expires_at: Option<String>,
    created_at: String,
}

fn normalize_ban_type(input: Option<String>) -> String {
    match input.unwrap_or_else(|| "soft".to_string()).trim().to_lowercase().as_str() {
        "hard" => "hard".to_string(),
        _ => "soft".to_string(),
    }
}

pub async fn block_user(
    State(st): State<CreatorBlockState>,
    cookies: Cookies,
    Json(payload): Json<BlockUserPayload>,
) -> impl IntoResponse {
    let (creator_id, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let blocked_user_id = payload.user_id.trim().to_string();
    if blocked_user_id.is_empty() {
        return Json(json!({"ok": false, "error": "user_id required"}));
    }
    if blocked_user_id == creator_id {
        return Json(json!({"ok": false, "error": "cannot block yourself"}));
    }

    let target_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = $1")
        .bind(&blocked_user_id)
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);
    if target_exists == 0 {
        return Json(json!({"ok": false, "error": "user not found"}));
    }

    let ban_type = normalize_ban_type(payload.ban_type);
    let res = sqlx::query(
        r#"
        INSERT INTO creator_blocked_users
            (creator_user_id, blocked_user_id, ban_type, reason, updated_at)
        VALUES ($1, $2, $3, $4, NOW())
        ON CONFLICT (creator_user_id, blocked_user_id)
        DO UPDATE SET
            ban_type = EXCLUDED.ban_type,
            reason = EXCLUDED.reason,
            expires_at = NULL,
            updated_at = NOW()
        "#,
    )
    .bind(&creator_id)
    .bind(&blocked_user_id)
    .bind(&ban_type)
    .bind(payload.reason.as_deref())
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => Json(json!({"ok": true, "blocked_user_id": blocked_user_id, "ban_type": ban_type})),
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn unblock_user(
    State(st): State<CreatorBlockState>,
    cookies: Cookies,
    Json(payload): Json<UnblockUserPayload>,
) -> impl IntoResponse {
    let (creator_id, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let blocked_user_id = payload.user_id.trim().to_string();
    if blocked_user_id.is_empty() {
        return Json(json!({"ok": false, "error": "user_id required"}));
    }

    let res = sqlx::query(
        "DELETE FROM creator_blocked_users WHERE creator_user_id = $1 AND blocked_user_id = $2",
    )
    .bind(&creator_id)
    .bind(&blocked_user_id)
    .execute(&st.pool)
    .await;

    match res {
        Ok(r) => Json(json!({"ok": true, "removed": r.rows_affected()})),
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn list_blocked_users(
    State(st): State<CreatorBlockState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let (creator_id, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let rows = sqlx::query(
        r#"
        SELECT
            cbu.blocked_user_id,
            COALESCE(u.username, '') AS username,
            COALESCE(u.email, '') AS email,
            cbu.ban_type,
            cbu.reason,
            cbu.expires_at::TEXT AS expires_at,
            cbu.created_at::TEXT AS created_at
        FROM creator_blocked_users cbu
        JOIN users u ON u.id = cbu.blocked_user_id
        WHERE cbu.creator_user_id = $1
          AND (cbu.expires_at IS NULL OR cbu.expires_at > NOW())
        ORDER BY cbu.created_at DESC
        "#,
    )
    .bind(&creator_id)
    .fetch_all(&st.pool)
    .await;

    let rows = match rows {
        Ok(v) => v,
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let items: Vec<BlockedUserItem> = rows
        .into_iter()
        .map(|r| BlockedUserItem {
            blocked_user_id: r.try_get("blocked_user_id").unwrap_or_default(),
            username: r.try_get("username").unwrap_or_default(),
            email: r.try_get("email").unwrap_or_default(),
            ban_type: r.try_get("ban_type").unwrap_or_default(),
            reason: r.try_get("reason").unwrap_or(None),
            expires_at: r.try_get("expires_at").unwrap_or(None),
            created_at: r.try_get::<Option<String>, _>("created_at").unwrap_or(None).unwrap_or_default(),
        })
        .collect();

    Json(json!({"ok": true, "items": items}))
}
