//src/handlers/users.rs

use axum::{extract::{Query, State}, response::IntoResponse, Json, Form};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower_cookies::Cookies;

use crate::sessions;

#[derive(Clone)]
pub struct UsersState { pub pool: PgPool }

#[derive(Serialize)]
pub struct PublicProfile {
    pub id: String,
    pub username: String,
    pub email: String,
    pub bank_account: Option<String>,
    pub wallet_account: Option<String>,
    pub whatsapp: Option<String>,
    pub profile_desc: String,
}

#[derive(Serialize)]
pub struct MeProfile {
    pub id: String,
    pub username: String,
    pub email: String,
    pub bank_account: String,
    pub wallet_account: String,
    pub whatsapp: String,
    pub profile_desc: String,
}

pub async fn get_my_profile(State(st): State<UsersState>, cookies: Cookies) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &cookies).await {
        Some(v) => v,
        None => return Json(serde_json::json!({"ok": false, "error": "not logged in"})),
    };

    let row = match sqlx::query!(
        r#"
        SELECT id, username, email,
               COALESCE(bank_account,'') as bank_account,
               COALESCE(wallet_account,'') as wallet_account,
               COALESCE(whatsapp,'') as whatsapp,
               COALESCE(profile_desc,'') as profile_desc
        FROM users
        WHERE id = $1
        LIMIT 1
        "#,
        uid
    ).fetch_optional(&st.pool).await {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    if let Some(u) = row {
        Json(serde_json::json!({
            "ok": true,
            "profile": MeProfile{
                id: u.id,
                username: u.username,
                email: u.email,
                bank_account: u.bank_account.unwrap_or_default(),
                wallet_account: u.wallet_account.unwrap_or_default(),
                whatsapp: u.whatsapp.unwrap_or_default(),
                profile_desc: u.profile_desc.unwrap_or_default(),
            }
        }))
    } else {
        Json(serde_json::json!({"ok": false, "error": "not found"}))
    }
}

#[derive(Deserialize)]
pub struct UpdateProfileForm {
    pub bank_account: String,
    pub wallet_account: String,
    pub whatsapp: String,
    pub profile_desc: String,
}

/// POST /api/profile_update (x-www-form-urlencoded)
pub async fn update_my_profile(
    State(st): State<UsersState>,
    cookies: Cookies,
    Form(f): Form<UpdateProfileForm>,
) -> impl IntoResponse {
    let (uid, _is_admin) = match sessions::current_user_id(&st.pool, &cookies).await {
        Some(v) => v,
        None => return Json(serde_json::json!({"ok": false, "where":"auth", "error": "not logged in"})),
    };

    let res = sqlx::query!(
        r#"
        UPDATE users
        SET bank_account = NULLIF($2,''),
            wallet_account = NULLIF($3,''),
            whatsapp = NULLIF($4,''),
            profile_desc = $5
        WHERE id = $1
        "#,
        uid,
        f.bank_account.trim(),
        f.wallet_account.trim(),
        f.whatsapp.trim(),
        f.profile_desc.trim()
    ).execute(&st.pool).await;

    match res {
        Ok(_) => Json(serde_json::json!({"ok": true})),
        Err(e) => Json(serde_json::json!({"ok": false, "where":"db_update", "error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct PublicQs {
    pub user_id: Option<String>,
    pub username: Option<String>,
}

/// GET /api/user_profile?user_id=... | ?username=...
pub async fn public_profile(State(st): State<UsersState>, Query(q): Query<PublicQs>) -> impl IntoResponse {
    if q.user_id.is_none() && q.username.is_none() {
        return Json(serde_json::json!({"ok": false, "error":"missing user_id/username"}));
    }

    let row = sqlx::query!(
        r#"
        SELECT id, username, email,
               bank_account, wallet_account, whatsapp,
               COALESCE(profile_desc,'') AS profile_desc
        FROM users
        WHERE ($1::text IS NOT NULL AND id = $1)
           OR ($2::text IS NOT NULL AND username = $2)
        LIMIT 1
        "#,
        q.user_id,
        q.username
    )
    .fetch_optional(&st.pool)
    .await;

    let row = match row {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("db: {e}")})),
    };

    if let Some(u) = row {
        Json(serde_json::json!({"ok": true, "profile": PublicProfile{
            id: u.id,
            username: u.username,
            email: u.email,
            bank_account: u.bank_account,
            wallet_account: u.wallet_account,
            whatsapp: u.whatsapp,
            profile_desc: u.profile_desc.unwrap_or_default(),
        }}))
    } else {
        Json(serde_json::json!({"ok": false, "error": "not found"}))
    }
}
