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
    pub cfg:  Config,
}

#[derive(Serialize)]
pub struct VideoItem {
    pub id: String,
    pub owner_id: String,
    pub owner_name: String,
    pub owner_email: String,
    pub owner_whatsapp: Option<String>,
    pub owner_wallet: Option<String>,
    pub owner_bank: Option<String>,
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
          u.email                      AS owner_email,
          u.whatsapp                   AS owner_whatsapp,
          u.wallet_account             AS owner_wallet,
          u.bank_account               AS owner_bank,
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
    .await {
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
            owner_email: r
                .try_get::<Option<String>, _>("owner_email")
                .unwrap_or(None)
                .unwrap_or_default(),
            owner_whatsapp: r.try_get::<Option<String>, _>("owner_whatsapp").ok().flatten(),
            owner_wallet: r.try_get::<Option<String>, _>("owner_wallet").ok().flatten(),
            owner_bank: r.try_get::<Option<String>, _>("owner_bank").ok().flatten(),
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
        let allow_rows = sqlx::query(
            r#"SELECT username FROM allowlist WHERE video_id = $1 ORDER BY username"#,
        )
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
    Query(qs): Query<UserLookupQs>,
) -> impl IntoResponse {
    if let Some(u) = qs.username.as_deref().filter(|s| !s.is_empty()) {
        let row = sqlx::query(r#"SELECT id, username, email FROM users WHERE username = $1 LIMIT 1"#)
            .bind(u)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
        return match row {
            Some(r) => Json(serde_json::json!({"ok": true, "user": {
                "id": r.try_get::<String,_>("id").unwrap_or_default(),
                "username": r.try_get::<String,_>("username").unwrap_or_default(),
                "email": r.try_get::<String,_>("email").unwrap_or_default(),
            }})),
            None => Json(serde_json::json!({"ok": true, "user": null})),
        };
    }
    if let Some(e) = qs.email.as_deref().filter(|s| !s.is_empty()) {
        let row = sqlx::query(r#"SELECT id, username, email FROM users WHERE email = $1 LIMIT 1"#)
            .bind(e)
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
        return match row {
            Some(r) => Json(serde_json::json!({"ok": true, "user": {
                "id": r.try_get::<String,_>("id").unwrap_or_default(),
                "username": r.try_get::<String,_>("username").unwrap_or_default(),
                "email": r.try_get::<String,_>("email").unwrap_or_default(),
            }})),
            None => Json(serde_json::json!({"ok": true, "user": null})),
        };
    }
    if let Some(q) = qs.q.as_deref().filter(|s| !s.is_empty()) {
        let pattern = format!("%{}%", q);
        let rows = sqlx::query(
            r#"
            SELECT id, username, email
            FROM users
            WHERE username ILIKE $1 OR email ILIKE $1
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
                    "email": r.try_get::<String,_>("email").unwrap_or_default(),
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
        match sqlx::query(
            r#"SELECT username FROM users WHERE username = $1 OR email = $1 LIMIT 1"#,
        )
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

    let res = sqlx::query(
        r#"
        UPDATE videos
        SET title = $2, description = $3, price_cents = $4
        WHERE id = $1 AND owner_id = $5
        "#,
    )
    .bind(&f.id)
    .bind(f.title.trim())
    .bind(f.description.trim())
    .bind(f.price_cents)
    .bind(&uid)
    .execute(&st.pool)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => Json(serde_json::json!({"ok": true})),
        Ok(_) => Json(serde_json::json!({"ok": false, "error": "not owner / not found"})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    }
}

/// AuthZ helper
pub async fn user_has_view_access(
    pool: &PgPool,
    video_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
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

    let username_opt = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT username FROM users WHERE id = $1"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(None);

    let Some(username) = username_opt else {
        return Ok(false);
    };

    let is_allowed: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM allowlist WHERE video_id = $1 AND username = $2)"#,
    )
    .bind(video_id)
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    Ok(is_allowed)
}
