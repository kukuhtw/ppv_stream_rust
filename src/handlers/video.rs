// src/handlers/video.rs
// src/handlers/video.rs
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Form, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use tower_cookies::Cookies;

use crate::config::Config;
use crate::sessions;

#[derive(Clone)]
pub struct VideoState {
    pub pool: PgPool,
    pub cfg: Config,
}

#[derive(Serialize)]
pub struct VideoItem {
    pub id: String,
    pub owner_id: String,
    pub owner_name: String,
    pub owner_profile_desc: String,
    pub title: String,
    pub description: String,
    pub price_cents: i64,
    pub filename: String,
    pub created_at: String,
}

pub async fn list_videos(State(st): State<VideoState>) -> impl IntoResponse {
    let rows = match sqlx::query(
        r#"
        SELECT
          v.id,
          v.owner_id,
          COALESCE(u.username, '(tidak diketahui)') AS owner_name,
          COALESCE(u.profile_desc,'')  AS owner_profile_desc,
          v.title,
          COALESCE(v.description, '')  AS description,
          v.price_cents,
          v.filename,
          v.created_at::text           AS created_at
        FROM videos v
        LEFT JOIN users u ON u.id = v.owner_id
        ORDER BY v.created_at DESC
        "#,
    )
    .fetch_all(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    let list: Vec<VideoItem> = rows
        .into_iter()
        .map(|r| VideoItem {
            id: r.try_get::<String, _>("id").unwrap_or_default(),
            owner_id: r.try_get::<String, _>("owner_id").unwrap_or_default(),
            owner_name: r
                .try_get::<String, _>("owner_name")
                .unwrap_or_else(|_| "(tidak diketahui)".to_string()),
            owner_profile_desc: r
                .try_get::<Option<String>, _>("owner_profile_desc")
                .ok()
                .flatten()
                .unwrap_or_default(),
            title: r.try_get::<String, _>("title").unwrap_or_default(),
            description: r.try_get::<String, _>("description").unwrap_or_default(),
            price_cents: r.try_get::<i64, _>("price_cents").unwrap_or(0),
            filename: r.try_get::<String, _>("filename").unwrap_or_default(),
            created_at: r.try_get::<String, _>("created_at").unwrap_or_default(),
        })
        .collect();

    Json(serde_json::json!({ "ok": true, "videos": list }))
}

#[derive(Serialize)]
struct MyVideo {
    id: String,
    title: String,
    description: String,
    price_cents: i64,
    created_at: String,
    allow_count: usize,
    allow_users: Vec<String>,
}

pub async fn my_videos(State(st): State<VideoState>, cookies: Cookies) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(serde_json::json!({"ok": false, "error": "not logged in"})),
    };

    let vids = match sqlx::query(
        r#"
        SELECT id, title, COALESCE(description,'') AS description,
               price_cents, created_at::text AS created_at
        FROM videos
        WHERE owner_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(&uid)
    .fetch_all(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    let mut out: Vec<MyVideo> = Vec::with_capacity(vids.len());
    for v in vids {
        let vid = v.try_get::<String, _>("id").unwrap_or_default();
        let allow_rows =
            sqlx::query(r#"SELECT username FROM allowlist WHERE video_id = $1 ORDER BY username"#)
                .bind(&vid)
                .fetch_all(&st.pool)
                .await
                .unwrap_or_default();

        let allow_users: Vec<String> = allow_rows
            .into_iter()
            .filter_map(|r| r.try_get::<Option<String>, _>("username").ok().flatten())
            .collect();

        out.push(MyVideo {
            id: vid,
            title: v.try_get::<String, _>("title").unwrap_or_default(),
            description: v.try_get::<String, _>("description").unwrap_or_default(),
            price_cents: v.try_get::<i64, _>("price_cents").unwrap_or(0),
            created_at: v.try_get::<String, _>("created_at").unwrap_or_default(),
            allow_count: allow_users.len(),
            allow_users,
        });
    }

    Json(serde_json::json!({ "ok": true, "videos": out }))
}

#[derive(Deserialize)]
pub struct UserLookupQs {
    pub q: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
}

pub async fn user_lookup(
    State(st): State<VideoState>,
    cookies: Cookies,
    Query(qs): Query<UserLookupQs>,
) -> impl IntoResponse {
    if sessions::current_user_id(&st.pool, &st.cfg, &cookies)
        .await
        .is_none()
    {
        return Json(serde_json::json!({"ok": false, "error": "not logged in"}));
    }

    if let Some(u) = qs.username.as_deref().filter(|s| !s.is_empty()) {
        let row = sqlx::query(r#"SELECT id, username FROM users WHERE username = $1 LIMIT 1"#)
            .bind(u)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
        return match row {
            Some(r) => Json(serde_json::json!({"ok": true, "user": {
                "id": r.try_get::<String,_>("id").unwrap_or_default(),
                "username": r.try_get::<String,_>("username").unwrap_or_default(),
            }})),
            None => Json(serde_json::json!({"ok": true, "user": null})),
        };
    }
    if let Some(e) = qs.email.as_deref().filter(|s| !s.is_empty()) {
        let _ = e;
        return Json(serde_json::json!({
            "ok": false,
            "error": "email lookup is disabled"
        }));
    }
    if let Some(q) = qs.q.as_deref().filter(|s| !s.is_empty()) {
        let pattern = format!("%{}%", q);
        let rows = sqlx::query(
            r#"
            SELECT id, username
            FROM users
            WHERE username ILIKE $1
            ORDER BY username
            LIMIT 20
            "#,
        )
        .bind(&pattern)
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default();

        let users: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.try_get::<String,_>("id").unwrap_or_default(),
                    "username": r.try_get::<String,_>("username").unwrap_or_default(),
                })
            })
            .collect();

        return Json(serde_json::json!({"ok": true, "users": users}));
    }
    Json(serde_json::json!({"ok": true, "users": []}))
}

#[derive(Deserialize)]
pub struct AddAllowForm {
    pub video_id: String,
    pub user_id: Option<String>,
    pub username_or_email: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
}

pub async fn add_allow(
    State(st): State<VideoState>,
    cookies: Cookies,
    Form(f): Form<AddAllowForm>,
) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(serde_json::json!({"ok": false, "error": "not logged in"})),
    };

    let is_owner: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM videos WHERE id = $1 AND owner_id = $2)"#,
    )
    .bind(&f.video_id)
    .bind(&uid)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(false);
    if !is_owner {
        return Json(serde_json::json!({"ok": false, "error": "not owner"}));
    }

    let target_uname = if let Some(u) = &f.username {
        u.trim().to_string()
    } else if let Some(e) = &f.email {
        match sqlx::query(r#"SELECT username FROM users WHERE email = $1 LIMIT 1"#)
            .bind(e.trim())
            .fetch_optional(&st.pool)
            .await
        {
            Ok(Some(r)) => r.try_get::<String, _>("username").unwrap_or_default(),
            _ => String::new(),
        }
    } else if let Some(id) = &f.user_id {
        match sqlx::query(r#"SELECT username FROM users WHERE id = $1 LIMIT 1"#)
            .bind(id.trim())
            .fetch_optional(&st.pool)
            .await
        {
            Ok(Some(r)) => r.try_get::<String, _>("username").unwrap_or_default(),
            _ => String::new(),
        }
    } else if let Some(k) = &f.username_or_email {
        match sqlx::query(r#"SELECT username FROM users WHERE username = $1 OR email = $1 LIMIT 1"#)
            .bind(k.trim())
            .fetch_optional(&st.pool)
            .await
        {
            Ok(Some(r)) => r.try_get::<String, _>("username").unwrap_or_default(),
            _ => String::new(),
        }
    } else {
        String::new()
    };

    if target_uname.is_empty() {
        return Json(serde_json::json!({"ok": false, "error": "user not found"}));
    }

    let res = sqlx::query(
        r#"
        INSERT INTO allowlist (video_id, username)
        VALUES ($1, $2)
        ON CONFLICT (video_id, username) DO NOTHING
        "#,
    )
    .bind(&f.video_id)
    .bind(&target_uname)
    .execute(&st.pool)
    .await;

    match res {
        Ok(r) => Json(serde_json::json!({
            "ok": true,
            "video_id": f.video_id,
            "username": target_uname,
            "rows_affected": r.rows_affected()
        })),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct UpdateVideoForm {
    pub id: String,
    pub title: String,
    pub description: String,
    pub price_cents: i64,
    /// Optional federation visibility toggle: `"public"` or `"local_only"`.
    /// Omitting the field leaves the current value unchanged.
    #[serde(default)]
    pub federation_visibility: Option<String>,
}

pub async fn update_video(
    State(st): State<VideoState>,
    cookies: Cookies,
    Form(f): Form<UpdateVideoForm>,
) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => return Json(serde_json::json!({"ok": false, "error": "not logged in"})),
    };

    // Validate and normalise the requested visibility change.
    // `None` (field absent) → leave unchanged; `Some("")` → also leave unchanged.
    let new_vis: Option<&str> = match f.federation_visibility.as_deref() {
        Some("public") => Some("public"),
        Some("local_only") => Some("local_only"),
        None | Some("") => None,
        Some(_) => {
            return Json(serde_json::json!({
                "ok": false,
                "error": "federation_visibility must be 'local_only' or 'public'"
            }));
        }
    };

    // When federation is active, snapshot the video's current visibility before
    // the update so we know which AP activity to broadcast afterwards.
    let prev_vis: String = if federation_enabled() {
        sqlx::query_scalar::<_, String>(
            "SELECT federation_visibility FROM videos \
             WHERE id = $1 AND owner_id = $2 LIMIT 1",
        )
        .bind(&f.id)
        .bind(&uid)
        .fetch_optional(&st.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| "local_only".to_string())
    } else {
        String::new()
    };

    let res = sqlx::query(
        r#"
        UPDATE videos
        SET title = $2, description = $3, price_cents = $4,
            federation_visibility = COALESCE($5, federation_visibility)
        WHERE id = $1 AND owner_id = $6
        "#,
    )
    .bind(&f.id)
    .bind(f.title.trim())
    .bind(f.description.trim())
    .bind(f.price_cents)
    .bind(new_vis)
    .bind(&uid)
    .execute(&st.pool)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => {
            // Determine effective new visibility (fall back to unchanged prev).
            let eff_new = new_vis.unwrap_or(prev_vis.as_str());
            let was_public = prev_vis == "public";
            let now_public = eff_new == "public";

            if federation_enabled() {
                let video_id = f.id.clone();
                let pool = st.pool.clone();
                let base_url = federation_base_url();
                tokio::spawn(async move {
                    let result = if !was_public && now_public {
                        crate::federation::video_index::publish_create(&pool, &video_id, &base_url)
                            .await
                            .map(|_| ())
                    } else if was_public && now_public {
                        crate::federation::video_index::publish_update(&pool, &video_id, &base_url)
                            .await
                    } else if was_public && !now_public {
                        crate::federation::video_index::publish_delete(&pool, &video_id, &base_url)
                            .await
                    } else {
                        return; // neither was nor is public — no federation action
                    };
                    if let Err(e) = result {
                        tracing::warn!(%video_id, "federation publish after update failed: {}", e);
                    }
                });
            }

            Json(serde_json::json!({"ok": true}))
        }
        Ok(_) => Json(serde_json::json!({"ok": false, "error": "not owner / not found"})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

fn federation_enabled() -> bool {
    std::env::var("FEDERATION_ENABLED")
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn federation_base_url() -> String {
    std::env::var("FEDERATION_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// AuthZ helper.
/// Grants access when any of these are true:
///   1. Video is free (price_cents = 0)
///   2. User is the video owner
///   3. User is an admin (is_admin session flag)
///   4. User is in the allowlist
///   5. User has a completed payment for the video
pub async fn user_has_view_access(
    pool: &PgPool,
    video_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    // 1. Free video — open to all authenticated users
    let price_cents: i64 =
        sqlx::query_scalar(r#"SELECT price_cents FROM videos WHERE id = $1 LIMIT 1"#)
            .bind(video_id)
            .fetch_one(pool)
            .await
            .unwrap_or(1); // default non-zero so we don't accidentally open paid videos
    if price_cents <= 0 {
        return Ok(true);
    }

    // 2. Owner
    let is_owner: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM videos WHERE id = $1 AND owner_id = $2)"#,
    )
    .bind(video_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if is_owner {
        return Ok(true);
    }

    // Fetch username + is_admin in one query for remaining checks
    let row = sqlx::query(r#"SELECT username, is_admin FROM users WHERE id = $1 LIMIT 1"#)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

    let Some(row) = row else {
        return Ok(false);
    };

    use sqlx::Row;
    let username: String = row.try_get("username").unwrap_or_default();
    // is_admin is stored as INTEGER 0/1
    let is_admin: bool = row.try_get::<i32, _>("is_admin").unwrap_or(0) != 0;

    // 3. Admin bypasses all access control
    if is_admin {
        return Ok(true);
    }

    // 4. Manual allowlist grant
    let is_allowed: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM allowlist WHERE video_id = $1 AND username = $2)"#,
    )
    .bind(video_id)
    .bind(&username)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if is_allowed {
        return Ok(true);
    }

    // 5. Completed payment: purchases table (x402 / wallet) or fiat_invoices (paid)
    let has_purchase: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM purchases WHERE video_id = $1 AND user_id = $2)"#,
    )
    .bind(video_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if has_purchase {
        return Ok(true);
    }

    let has_fiat_paid: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1 FROM fiat_invoices
            WHERE video_id = $1 AND user_id = $2 AND status = 'paid'
        )"#,
    )
    .bind(video_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    Ok(has_fiat_paid)
}
