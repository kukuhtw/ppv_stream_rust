// src/handlers/video.rs
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Form, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use tower_cookies::Cookies;

use crate::sessions;

#[derive(Clone)]
pub struct VideoState {
    pub pool: PgPool,
}

#[derive(Serialize)]
pub struct VideoItem {
    pub id: String,
    pub owner_id: String,
    pub owner_name: String, // nama owner untuk UI
    pub title: String,
    pub description: String, // ← NEW
    pub price_cents: i64,
    pub filename: String,
    pub created_at: String,
}

// GET /api/videos  → listing umum (dengan owner_name)
// GET /api/videos  → listing umum (dengan owner_name + description)
pub async fn list_videos(State(st): State<VideoState>) -> impl IntoResponse {
    let rows = match sqlx::query!(
        r#"
        SELECT
          v.id,
          v.owner_id,
          COALESCE(u.username, '(tidak diketahui)') AS owner_name,
          v.title,
          COALESCE(v.description, '') AS description,
          v.price_cents,
          v.filename,
          v.created_at
        FROM videos v
        LEFT JOIN users u ON u.id = v.owner_id
        ORDER BY v.created_at DESC
        "#
    )
    .fetch_all(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")}));
        }
    };

    let list: Vec<VideoItem> = rows
        .into_iter()
        .map(|r| VideoItem {
            id: r.id,
            owner_id: r.owner_id,
            owner_name: r.owner_name.unwrap_or_else(|| "(tidak diketahui)".to_string()),
            title: r.title,
            description: r.description.unwrap_or_default(),
            price_cents: r.price_cents,
            filename: r.filename,
            created_at: r.created_at,
        })
        .collect();

    Json(serde_json::json!({ "ok": true, "videos": list }))
}


// ==== MY VIDEOS (dengan allowlist) ====

#[derive(Serialize)]
struct MyVideo {
    id: String,
    title: String,
    description: String, // ← NEW
    price_cents: i64,
    created_at: String,
    allow_count: usize,
    allow_users: Vec<String>,
}

// GET /api/my_videos → butuh login
// GET /api/my_videos → butuh login
pub async fn my_videos(State(st): State<VideoState>, cookies: Cookies) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &cookies).await {
        Some(v) => v,
        None => return Json(serde_json::json!({"ok": false, "error": "not logged in"})),
    };

    let vids = match sqlx::query!(
        r#"
        SELECT id, title, COALESCE(description,'') AS description, price_cents, created_at
        FROM videos
        WHERE owner_id = $1
        ORDER BY created_at DESC
        "#,
        uid
    )
    .fetch_all(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    if vids.is_empty() {
        return Json(serde_json::json!({ "ok": true, "videos": [] }));
    }

    let ids: Vec<String> = vids.iter().map(|r| r.id.clone()).collect();

    let allow_rows = match sqlx::query!(
        r#"
        SELECT video_id, username
        FROM allowlist
        WHERE video_id = ANY($1)
        "#,
        &ids[..]
    )
    .fetch_all(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db allow: {e}")})),
    };

    use std::collections::HashMap;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for ar in allow_rows {
        map.entry(ar.video_id).or_default().push(ar.username);
    }

    let out: Vec<MyVideo> = vids
        .into_iter()
        .map(|v| {
            let lst = map.remove(&v.id).unwrap_or_default();
            MyVideo {
                id: v.id,
                title: v.title,
                description: v.description.unwrap_or_default(),
                price_cents: v.price_cents,
                created_at: v.created_at,
                allow_count: lst.len(),
                allow_users: lst,
            }
        })
        .collect();

    Json(serde_json::json!({ "ok": true, "videos": out }))
}


// ==== USER LOOKUP ====

#[derive(Deserialize)]
pub struct LookupQs {
    pub username: String,
}

#[derive(Deserialize)]
pub struct UpdateVideoForm {
    pub video_id: String,
    pub title: String,
    pub description: String,
    pub price_cents: i64,
}

// POST /api/video_update (x-www-form-urlencoded)
// Hanya boleh oleh owner video.
pub async fn update_video(
    State(st): State<VideoState>,
    cookies: Cookies,
    Form(f): Form<UpdateVideoForm>,
) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &cookies).await {
        Some(v) => v,
        None => {
            return Json(serde_json::json!({"ok": false, "where": "auth", "error": "not logged in"}));
        }
    };

    let vid = f.video_id.trim();
    if vid.is_empty() {
        return Json(serde_json::json!({"ok": false, "where": "validation", "error": "video_id required"}));
    }
    if f.price_cents < 0 {
        return Json(serde_json::json!({"ok": false, "where": "validation", "error": "price_cents must be >= 0"}));
    }

    // Pastikan video milik user
    let owner_row = match sqlx::query_scalar::<_, String>(
        r#"SELECT owner_id FROM videos WHERE id = $1 LIMIT 1"#,
    )
    .bind(vid)
    .fetch_optional(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "where":"db_video", "error": e.to_string()})),
    };

    let Some(owner_id) = owner_row else {
        return Json(serde_json::json!({"ok": false, "where":"video", "error": "video not found"}));
    };
    if owner_id != uid {
        return Json(serde_json::json!({"ok": false, "where":"authz", "error": "not the owner"}));
    }

    // Update
    let res = sqlx::query!(
        r#"
        UPDATE videos
        SET title = $2,
            description = $3,
            price_cents = $4
        WHERE id = $1
        "#,
        vid,
        f.title.trim(),
        f.description.trim(),
        f.price_cents
    )
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => Json(serde_json::json!({"ok": true, "video_id": vid})),
        Err(e) => Json(serde_json::json!({"ok": false, "where":"db_update", "error": e.to_string()})),
    }
}


// GET /api/user_lookup?username=...
pub async fn user_lookup(
    State(st): State<VideoState>,
    Query(q): Query<LookupQs>,
) -> impl IntoResponse {
    let uname = q.username.trim();
    if uname.is_empty() {
        return Json(serde_json::json!({"ok": false, "error": "missing username"}));
    }

    let row = match sqlx::query!(
        r#"SELECT id, username, email FROM users WHERE username = $1 LIMIT 1"#,
        uname
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    if let Some(u) = row {
        Json(serde_json::json!({
            "ok": true,
            "user": { "id": u.id, "username": u.username, "email": u.email }
        }))
    } else {
        Json(serde_json::json!({ "ok": true, "user": serde_json::Value::Null }))
    }
}

// ==== ADD ALLOWLIST ====

#[derive(Deserialize)]
pub struct AllowForm {
    pub video_id: String,
    pub username: String,
}

// POST /api/allow (x-www-form-urlencoded)
pub async fn add_allow(
    State(st): State<VideoState>,
    cookies: Cookies,
    Form(f): Form<AllowForm>,
) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &cookies).await {
        Some(v) => v,
        None => {
            return Json(serde_json::json!({"ok": false, "where":"auth", "error": "not logged in"}))
        }
    };

    let vid = f.video_id.trim();
    let uname = f.username.trim();

    if vid.is_empty() || uname.is_empty() {
        return Json(
            serde_json::json!({"ok": false, "where":"validation", "error": "video_id/username required"}),
        );
    }

    // Pastikan video milik user
    let owner_row = match sqlx::query!(r#"SELECT owner_id FROM videos WHERE id = $1 LIMIT 1"#, vid)
        .fetch_optional(&st.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return Json(
                serde_json::json!({"ok": false, "where":"db_video", "error": e.to_string()}),
            )
        }
    };

    let Some(vrow) = owner_row else {
        return Json(serde_json::json!({"ok": false, "where":"video", "error": "video not found"}));
    };

    if vrow.owner_id != uid {
        return Json(serde_json::json!({"ok": false, "where":"authz", "error": "not the owner"}));
    }

    // Pastikan username ada (INT4/i32)
    let exists =
        match sqlx::query_scalar::<_, i32>(r#"SELECT 1 FROM users WHERE username = $1 LIMIT 1"#)
            .bind(uname)
            .fetch_optional(&st.pool)
            .await
        {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(e) => {
                return Json(
                    serde_json::json!({"ok": false, "where":"db_user", "error": e.to_string()}),
                )
            }
        };

    if !exists {
        return Json(
            serde_json::json!({"ok": false, "where":"user", "error": "username not found"}),
        );
    }

    // Tambahkan allowlist (idempotent)
    let res = sqlx::query!(
        r#"
        INSERT INTO allowlist (video_id, username)
        VALUES ($1, $2)
        ON CONFLICT (video_id, username) DO NOTHING
        "#,
        vid,
        uname
    )
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => Json(serde_json::json!({"ok": true, "video_id": vid, "username": uname})),
        Err(e) => {
            Json(serde_json::json!({"ok": false, "where":"db_insert", "error": e.to_string()}))
        }
    }
}

/// Util untuk modul streaming:
/// true jika:
/// - owner video, atau
/// - ada di allowlist (by username), atau
/// - ada purchase (opsional)
pub async fn user_has_view_access(
    pool: &PgPool,
    video_id: &str,
    user_id: &str,
) -> sqlx::Result<bool> {
    // owner?
    if let Some(owner) =
        sqlx::query_scalar::<_, String>(r#"SELECT owner_id FROM videos WHERE id = $1 LIMIT 1"#)
            .bind(video_id)
            .fetch_optional(pool)
            .await?
    {
        if owner == user_id {
            return Ok(true);
        }
    } else {
        return Ok(false);
    }

    // username dari user_id
    let username = match sqlx::query_scalar::<_, String>(
        r#"SELECT username FROM users WHERE id = $1 LIMIT 1"#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    {
        Some(u) => u,
        None => return Ok(false),
    };

    // allowlist? (INT4/i32)
    if sqlx::query_scalar::<_, i32>(
        r#"SELECT 1 FROM allowlist WHERE video_id = $1 AND username = $2 LIMIT 1"#,
    )
    .bind(video_id)
    .bind(&username)
    .fetch_optional(pool)
    .await?
    .is_some()
    {
        return Ok(true);
    }

    // purchases? (opsional) (INT4/i32)
    if sqlx::query_scalar::<_, i32>(
        r#"SELECT 1 FROM purchases WHERE video_id = $1 AND user_id = $2 LIMIT 1"#,
    )
    .bind(video_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .is_some()
    {
        return Ok(true);
    }

    Ok(false)
}
