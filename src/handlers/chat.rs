use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::config::Config;
use crate::sessions;

#[derive(Clone)]
pub struct ChatState {
    pub pool: PgPool,
    pub cfg: Config,
}

#[derive(Serialize)]
struct ChatUserSummary {
    id: String,
    username: String,
    email: String,
}

#[derive(Serialize)]
struct ConversationSummary {
    id: String,
    conversation_type: String,
    title: String,
    counterpart: Option<ChatUserSummary>,
    last_message: String,
    last_message_at: String,
    last_sender_name: String,
    created_at: String,
}

#[derive(Serialize)]
struct MessageItem {
    id: String,
    sender_user_id: String,
    sender_username: String,
    sender_is_admin: bool,
    body: String,
    created_at: String,
    is_self: bool,
}

#[derive(Deserialize)]
pub struct ChatUserSearchQuery {
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct StartDirectConversationPayload {
    pub target_user_id: String,
}

#[derive(Deserialize)]
pub struct SendMessagePayload {
    pub body: String,
}

async fn current_actor(
    st: &ChatState,
    cookies: &Cookies,
) -> Result<(String, bool, String), serde_json::Value> {
    let Some((user_id, is_admin)) = sessions::current_user_id(&st.pool, &st.cfg, cookies).await
    else {
        return Err(json!({"ok": false, "error": "not logged in"}));
    };

    let row = sqlx::query("SELECT username FROM users WHERE id = $1 LIMIT 1")
        .bind(&user_id)
        .fetch_optional(&st.pool)
        .await;

    match row {
        Ok(Some(r)) => Ok((
            user_id,
            is_admin,
            r.try_get::<String, _>("username").unwrap_or_default(),
        )),
        _ => Err(json!({"ok": false, "error": "user not found"})),
    }
}

async fn user_can_access_conversation(
    pool: &PgPool,
    conversation_id: &str,
    user_id: &str,
    is_admin: bool,
) -> anyhow::Result<bool> {
    let row = sqlx::query(
        r#"
        SELECT conversation_type, direct_user_a_id, direct_user_b_id, support_user_id
        FROM chat_conversations
        WHERE id = $1
        LIMIT 1
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else {
        return Ok(false);
    };

    let convo_type: String = r.try_get("conversation_type").unwrap_or_default();
    let direct_a: Option<String> = r.try_get("direct_user_a_id").unwrap_or(None);
    let direct_b: Option<String> = r.try_get("direct_user_b_id").unwrap_or(None);
    let support_user_id: Option<String> = r.try_get("support_user_id").unwrap_or(None);

    let allowed = match convo_type.as_str() {
        "admin_support" => is_admin || support_user_id.as_deref() == Some(user_id),
        "direct" => direct_a.as_deref() == Some(user_id) || direct_b.as_deref() == Some(user_id),
        _ => false,
    };

    Ok(allowed)
}

async fn get_or_create_support_conversation(
    pool: &PgPool,
    support_user_id: &str,
) -> anyhow::Result<String> {
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM chat_conversations
        WHERE conversation_type = 'admin_support' AND support_user_id = $1
        LIMIT 1
        "#,
    )
    .bind(support_user_id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(existing);
    }

    let now = Utc::now().to_rfc3339();
    let conversation_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO chat_conversations
          (id, conversation_type, support_user_id, created_by_user_id, created_at, updated_at, last_message_at)
        VALUES
          ($1, 'admin_support', $2, $2, $3, $3, $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(&conversation_id)
    .bind(support_user_id)
    .bind(&now)
    .execute(pool)
    .await?;

    let actual_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM chat_conversations
        WHERE conversation_type = 'admin_support' AND support_user_id = $1
        LIMIT 1
        "#,
    )
    .bind(support_user_id)
    .fetch_one(pool)
    .await?;

    Ok(actual_id)
}

pub async fn search_chat_users(
    State(st): State<ChatState>,
    cookies: Cookies,
    Query(q): Query<ChatUserSearchQuery>,
) -> impl IntoResponse {
    let (user_id, is_admin, _) = match current_actor(&st, &cookies).await {
        Ok(v) => v,
        Err(err) => return Json(err),
    };

    let needle = q.q.unwrap_or_default().trim().to_string();
    if needle.is_empty() {
        return Json(json!({"ok": true, "users": []}));
    }

    let pattern = format!("%{}%", needle);
    let rows = sqlx::query(
        r#"
        SELECT id, username, COALESCE(email, '') AS email
        FROM users
        WHERE is_admin = 0
          AND id <> $1
          AND (username ILIKE $2 OR email ILIKE $2)
        ORDER BY username
        LIMIT 20
        "#,
    )
    .bind(&user_id)
    .bind(&pattern)
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    let users: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.try_get::<String, _>("id").unwrap_or_default(),
                "username": r.try_get::<String, _>("username").unwrap_or_default(),
                "email": r.try_get::<String, _>("email").unwrap_or_default(),
                "can_start_direct": !is_admin,
            })
        })
        .collect();

    Json(json!({"ok": true, "users": users}))
}

pub async fn ensure_support_conversation(
    State(st): State<ChatState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let (user_id, is_admin, _) = match current_actor(&st, &cookies).await {
        Ok(v) => v,
        Err(err) => return Json(err),
    };

    if is_admin {
        return Json(json!({"ok": false, "error": "admins cannot create support conversations"}));
    }

    match get_or_create_support_conversation(&st.pool, &user_id).await {
        Ok(id) => Json(json!({"ok": true, "conversation_id": id})),
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn start_direct_conversation(
    State(st): State<ChatState>,
    cookies: Cookies,
    Json(payload): Json<StartDirectConversationPayload>,
) -> impl IntoResponse {
    let (user_id, is_admin, _) = match current_actor(&st, &cookies).await {
        Ok(v) => v,
        Err(err) => return Json(err),
    };

    if is_admin {
        return Json(
            json!({"ok": false, "error": "admin cannot start direct user chats from this endpoint"}),
        );
    }

    let target_user_id = payload.target_user_id.trim().to_string();
    if target_user_id.is_empty() || target_user_id == user_id {
        return Json(json!({"ok": false, "error": "invalid target user"}));
    }

    let target = sqlx::query(
        r#"
        SELECT id, is_admin
        FROM users
        WHERE id = $1
        LIMIT 1
        "#,
    )
    .bind(&target_user_id)
    .fetch_optional(&st.pool)
    .await;

    let Some(target) = target.ok().flatten() else {
        return Json(json!({"ok": false, "error": "target user not found"}));
    };
    let target_is_admin = target.try_get::<i32, _>("is_admin").unwrap_or(0) != 0;
    if target_is_admin {
        return Json(json!({"ok": false, "error": "use support chat to contact admin"}));
    }

    let (user_a, user_b) = if user_id <= target_user_id {
        (user_id.clone(), target_user_id.clone())
    } else {
        (target_user_id.clone(), user_id.clone())
    };

    if let Some(existing) = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM chat_conversations
        WHERE conversation_type = 'direct'
          AND direct_user_a_id = $1
          AND direct_user_b_id = $2
        LIMIT 1
        "#,
    )
    .bind(&user_a)
    .bind(&user_b)
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None)
    {
        return Json(json!({"ok": true, "conversation_id": existing}));
    }

    let now = Utc::now().to_rfc3339();
    let conversation_id = Uuid::new_v4().to_string();
    let res = sqlx::query(
        r#"
        INSERT INTO chat_conversations
          (id, conversation_type, direct_user_a_id, direct_user_b_id, created_by_user_id, created_at, updated_at, last_message_at)
        VALUES
          ($1, 'direct', $2, $3, $4, $5, $5, $5)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(&conversation_id)
    .bind(&user_a)
    .bind(&user_b)
    .bind(&user_id)
    .bind(&now)
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => {
            let actual_id = sqlx::query_scalar::<_, String>(
                r#"
                SELECT id
                FROM chat_conversations
                WHERE conversation_type = 'direct'
                  AND direct_user_a_id = $1
                  AND direct_user_b_id = $2
                LIMIT 1
                "#,
            )
            .bind(&user_a)
            .bind(&user_b)
            .fetch_one(&st.pool)
            .await;

            match actual_id {
                Ok(id) => Json(json!({"ok": true, "conversation_id": id})),
                Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
            }
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn list_conversations(
    State(st): State<ChatState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let (user_id, is_admin, _) = match current_actor(&st, &cookies).await {
        Ok(v) => v,
        Err(err) => return Json(err),
    };

    if !is_admin {
        let _ = get_or_create_support_conversation(&st.pool, &user_id).await;
    }

    let rows = if is_admin {
        sqlx::query(
            r#"
            SELECT
              cc.id,
              cc.conversation_type,
              cc.created_at::text AS created_at,
              cc.last_message_at::text AS last_message_at,
              u.id AS support_user_id,
              COALESCE(u.username, 'User') AS support_username,
              COALESCE(u.email, '') AS support_email
            FROM chat_conversations cc
            JOIN users u ON u.id = cc.support_user_id
            WHERE cc.conversation_type = 'admin_support'
            ORDER BY cc.last_message_at DESC, cc.created_at DESC
            "#,
        )
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query(
            r#"
            SELECT
              cc.id,
              cc.conversation_type,
              cc.created_at::text AS created_at,
              cc.last_message_at::text AS last_message_at,
              other.id AS other_user_id,
              COALESCE(other.username, 'User') AS other_username,
              COALESCE(other.email, '') AS other_email
            FROM chat_conversations cc
            LEFT JOIN users other
              ON other.id = CASE
                WHEN cc.conversation_type = 'direct' AND cc.direct_user_a_id = $1 THEN cc.direct_user_b_id
                WHEN cc.conversation_type = 'direct' AND cc.direct_user_b_id = $1 THEN cc.direct_user_a_id
                ELSE NULL
              END
            WHERE
              (cc.conversation_type = 'admin_support' AND cc.support_user_id = $1)
              OR
              (cc.conversation_type = 'direct' AND (cc.direct_user_a_id = $1 OR cc.direct_user_b_id = $1))
            ORDER BY cc.last_message_at DESC, cc.created_at DESC
            "#
        )
        .bind(&user_id)
        .fetch_all(&st.pool)
        .await
        .unwrap_or_default()
    };

    let mut conversations = Vec::with_capacity(rows.len());
    for r in rows {
        let conversation_id = r.try_get::<String, _>("id").unwrap_or_default();
        let convo_type = r
            .try_get::<String, _>("conversation_type")
            .unwrap_or_default();

        let latest = sqlx::query(
            r#"
            SELECT
              cm.body,
              cm.created_at::text AS created_at,
              COALESCE(u.username, 'Unknown') AS sender_username
            FROM chat_messages cm
            LEFT JOIN users u ON u.id = cm.sender_user_id
            WHERE cm.conversation_id = $1
            ORDER BY cm.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(&conversation_id)
        .fetch_optional(&st.pool)
        .await
        .unwrap_or(None);

        let (last_message, last_sender_name, last_message_at) = if let Some(m) = latest {
            (
                m.try_get::<String, _>("body").unwrap_or_default(),
                m.try_get::<String, _>("sender_username")
                    .unwrap_or_else(|_| "Unknown".to_string()),
                m.try_get::<String, _>("created_at").unwrap_or_default(),
            )
        } else {
            (
                String::new(),
                String::new(),
                r.try_get::<String, _>("last_message_at")
                    .unwrap_or_default(),
            )
        };

        let (title, counterpart) = if convo_type == "admin_support" {
            if is_admin {
                let support_id = r
                    .try_get::<String, _>("support_user_id")
                    .unwrap_or_default();
                let support_username = r
                    .try_get::<String, _>("support_username")
                    .unwrap_or_else(|_| "User".to_string());
                let support_email = r.try_get::<String, _>("support_email").unwrap_or_default();
                (
                    format!("Support: {}", support_username),
                    Some(ChatUserSummary {
                        id: support_id,
                        username: support_username,
                        email: support_email,
                    }),
                )
            } else {
                ("Chat with Admin".to_string(), None)
            }
        } else {
            let other_id = r.try_get::<String, _>("other_user_id").unwrap_or_default();
            let other_username = r
                .try_get::<String, _>("other_username")
                .unwrap_or_else(|_| "User".to_string());
            let other_email = r.try_get::<String, _>("other_email").unwrap_or_default();
            (
                other_username.clone(),
                Some(ChatUserSummary {
                    id: other_id,
                    username: other_username,
                    email: other_email,
                }),
            )
        };

        conversations.push(ConversationSummary {
            id: conversation_id,
            conversation_type: convo_type,
            title,
            counterpart,
            last_message,
            last_message_at,
            last_sender_name,
            created_at: r.try_get::<String, _>("created_at").unwrap_or_default(),
        });
    }

    Json(json!({"ok": true, "conversations": conversations}))
}

pub async fn list_messages(
    State(st): State<ChatState>,
    cookies: Cookies,
    Path(conversation_id): Path<String>,
) -> impl IntoResponse {
    let (user_id, is_admin, _) = match current_actor(&st, &cookies).await {
        Ok(v) => v,
        Err(err) => return Json(err),
    };

    match user_can_access_conversation(&st.pool, &conversation_id, &user_id, is_admin).await {
        Ok(true) => {}
        Ok(false) => return Json(json!({"ok": false, "error": "forbidden"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    }

    let rows = sqlx::query(
        r#"
        SELECT
          cm.id,
          cm.sender_user_id,
          COALESCE(u.username, 'Unknown') AS sender_username,
          COALESCE(u.is_admin, 0) AS sender_is_admin,
          cm.body,
          cm.created_at::text AS created_at
        FROM chat_messages cm
        LEFT JOIN users u ON u.id = cm.sender_user_id
        WHERE cm.conversation_id = $1
        ORDER BY cm.created_at ASC
        "#,
    )
    .bind(&conversation_id)
    .fetch_all(&st.pool)
    .await;

    match rows {
        Ok(rows) => {
            let messages: Vec<MessageItem> = rows
                .into_iter()
                .map(|r| MessageItem {
                    id: r.try_get::<String, _>("id").unwrap_or_default(),
                    sender_user_id: r.try_get::<String, _>("sender_user_id").unwrap_or_default(),
                    sender_username: r
                        .try_get::<String, _>("sender_username")
                        .unwrap_or_else(|_| "Unknown".to_string()),
                    sender_is_admin: r.try_get::<i32, _>("sender_is_admin").unwrap_or(0) != 0,
                    body: r.try_get::<String, _>("body").unwrap_or_default(),
                    created_at: r.try_get::<String, _>("created_at").unwrap_or_default(),
                    is_self: r.try_get::<String, _>("sender_user_id").unwrap_or_default()
                        == user_id,
                })
                .collect();
            Json(json!({"ok": true, "messages": messages}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

pub async fn send_message(
    State(st): State<ChatState>,
    cookies: Cookies,
    Path(conversation_id): Path<String>,
    Json(payload): Json<SendMessagePayload>,
) -> impl IntoResponse {
    let (user_id, is_admin, sender_username) = match current_actor(&st, &cookies).await {
        Ok(v) => v,
        Err(err) => return Json(err),
    };

    match user_can_access_conversation(&st.pool, &conversation_id, &user_id, is_admin).await {
        Ok(true) => {}
        Ok(false) => return Json(json!({"ok": false, "error": "forbidden"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    }

    let body = payload.body.trim();
    if body.is_empty() {
        return Json(json!({"ok": false, "error": "message cannot be empty"}));
    }
    if body.chars().count() > 4000 {
        return Json(json!({"ok": false, "error": "message too long"}));
    }

    let now = Utc::now().to_rfc3339();
    let message_id = Uuid::new_v4().to_string();
    let mut tx = match st.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let insert_res = sqlx::query(
        r#"
        INSERT INTO chat_messages (id, conversation_id, sender_user_id, body, created_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&message_id)
    .bind(&conversation_id)
    .bind(&user_id)
    .bind(body)
    .bind(&now)
    .execute(&mut *tx)
    .await;

    if let Err(e) = insert_res {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db: {e}")}));
    }

    if let Err(e) = sqlx::query(
        r#"
        UPDATE chat_conversations
        SET updated_at = $2, last_message_at = $2
        WHERE id = $1
        "#,
    )
    .bind(&conversation_id)
    .bind(&now)
    .execute(&mut *tx)
    .await
    {
        let _ = tx.rollback().await;
        return Json(json!({"ok": false, "error": format!("db: {e}")}));
    }

    if let Err(e) = tx.commit().await {
        return Json(json!({"ok": false, "error": format!("commit: {e}")}));
    }

    Json(json!({
        "ok": true,
        "message": {
            "id": message_id,
            "sender_user_id": user_id,
            "sender_username": sender_username,
            "sender_is_admin": is_admin,
            "body": body,
            "created_at": now,
            "is_self": true
        }
    }))
}
