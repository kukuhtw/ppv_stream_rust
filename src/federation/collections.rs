use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use super::{api_error, FederationState};

/// Maximum allowed size of an inbound ActivityPub payload.
const MAX_PAYLOAD_BYTES: usize = 64 * 1024; // 64 KB

// ── GET /users/:username/followers ─────────────────────────────────────────

pub async fn actor_followers(
    State(state): State<FederationState>,
    Path(username): Path<String>,
) -> Response {
    let actor_uri = format!("{}/users/{}", state.config.base_url, username);

    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM federation_follows ff
         JOIN federation_actors fa ON fa.id = ff.following_actor_id
         WHERE fa.actor_uri = $1 AND ff.status = 'accepted'",
    )
    .bind(&actor_uri)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT fa.actor_uri FROM federation_follows ff
         JOIN federation_actors fa ON fa.id = ff.follower_actor_id
         JOIN federation_actors target ON target.id = ff.following_actor_id
         WHERE target.actor_uri = $1 AND ff.status = 'accepted'
         ORDER BY ff.created_at DESC
         LIMIT 100",
    )
    .bind(&actor_uri)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let followers_url = format!("{}/followers", actor_uri);
    let items: Vec<Value> = rows.into_iter().map(|(uri,)| json!(uri)).collect();

    activitypub_response(json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": followers_url,
        "type": "OrderedCollection",
        "totalItems": total.unwrap_or(0),
        "orderedItems": items
    }))
}

// ── GET /users/:username/following ─────────────────────────────────────────

pub async fn actor_following(
    State(state): State<FederationState>,
    Path(username): Path<String>,
) -> Response {
    let actor_uri = format!("{}/users/{}", state.config.base_url, username);

    let total: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM federation_follows ff
         JOIN federation_actors fa ON fa.id = ff.follower_actor_id
         WHERE fa.actor_uri = $1 AND ff.status = 'accepted'",
    )
    .bind(&actor_uri)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT fa.actor_uri FROM federation_follows ff
         JOIN federation_actors fa ON fa.id = ff.following_actor_id
         JOIN federation_actors src ON src.id = ff.follower_actor_id
         WHERE src.actor_uri = $1 AND ff.status = 'accepted'
         ORDER BY ff.created_at DESC
         LIMIT 100",
    )
    .bind(&actor_uri)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let following_url = format!("{}/following", actor_uri);
    let items: Vec<Value> = rows.into_iter().map(|(uri,)| json!(uri)).collect();

    activitypub_response(json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": following_url,
        "type": "OrderedCollection",
        "totalItems": total.unwrap_or(0),
        "orderedItems": items
    }))
}

// ── GET /users/:username/outbox ────────────────────────────────────────────

pub async fn actor_outbox(
    State(state): State<FederationState>,
    Path(username): Path<String>,
) -> Response {
    let actor_uri = format!("{}/users/{}", state.config.base_url, username);

    let rows: Vec<(Value,)> = sqlx::query_as(
        "SELECT payload FROM federation_activities
         WHERE actor_uri = $1 AND direction = 'outbound'
           AND activity_type IN ('Create', 'Update', 'Announce')
           AND processing_status = 'processed'
         ORDER BY created_at DESC
         LIMIT 20",
    )
    .bind(&actor_uri)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let outbox_url = format!("{}/outbox", actor_uri);
    let items: Vec<Value> = rows.into_iter().map(|(payload,)| payload).collect();
    let total = items.len() as i64;

    activitypub_response(json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": outbox_url,
        "type": "OrderedCollection",
        "totalItems": total,
        "orderedItems": items
    }))
}

// ── GET /users/:username/inbox ─────────────────────────────────────────────

/// Returns 403 — actor inboxes are write-only from the network perspective.
pub async fn actor_inbox_get(
    State(_state): State<FederationState>,
    Path(_username): Path<String>,
) -> Response {
    api_error(StatusCode::FORBIDDEN, "actor inbox is not publicly readable")
}

// ── POST /users/:username/inbox ────────────────────────────────────────────

pub async fn actor_inbox_post(
    State(state): State<FederationState>,
    Path(username): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let actor_uri = format!("{}/users/{}", state.config.base_url, username);
    process_inbox(&state, Some(actor_uri), headers, body).await
}

// ── POST /inbox (shared inbox) ─────────────────────────────────────────────

pub async fn shared_inbox_post(
    State(state): State<FederationState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    process_inbox(&state, None, headers, body).await
}

// ── Inbox processing ───────────────────────────────────────────────────────

async fn process_inbox(
    state: &FederationState,
    target_actor_uri: Option<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // 1. Enforce payload size limit
    if body.len() > MAX_PAYLOAD_BYTES {
        return api_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload exceeds 64 KB federation limit",
        );
    }

    // 2. Parse the activity JSON
    let activity: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid JSON payload"),
    };

    let activity_uri = activity
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let activity_type = activity
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let actor_uri_in_activity = activity
        .get("actor")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // 3. Domain rule enforcement — reject activities from blocked/suspended domains
    {
        let actor_domain = actor_uri_in_activity
            .strip_prefix("https://")
            .or_else(|| actor_uri_in_activity.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("");

        if !actor_domain.is_empty()
            && crate::federation::moderation::is_domain_blocked(&state.pool, actor_domain).await
        {
            tracing::info!(
                actor = %actor_uri_in_activity,
                "inbox: rejected activity from blocked/suspended domain {}",
                actor_domain
            );
            return api_error(StatusCode::FORBIDDEN, "domain is blocked");
        }
    }

    // 4. Verify Digest header if present
    if let Some(digest_header) = headers.get("digest") {
        if let Ok(digest_str) = digest_header.to_str() {
            if let Err(e) = crate::federation::signatures::verify_digest(&body, digest_str) {
                tracing::warn!("digest mismatch from {}: {}", actor_uri_in_activity, e);
                return api_error(StatusCode::BAD_REQUEST, "Digest header mismatch");
            }
        }
    }

    // 5. Verify HTTP Signature
    let sig_header = match headers.get("signature").and_then(|v| v.to_str().ok()) {
        Some(s) => s.to_owned(),
        None => {
            return api_error(StatusCode::UNAUTHORIZED, "missing Signature header");
        }
    };

    // Collect request headers for signature verification
    let mut req_headers = std::collections::HashMap::<String, String>::new();
    for (name, value) in &headers {
        if let Ok(v) = value.to_str() {
            req_headers.insert(name.as_str().to_lowercase(), v.to_string());
        }
    }

    // Fetch the remote actor's public key (SSRF-safe)
    let actor_key_result =
        crate::federation::resolver::fetch_remote_actor_key(&actor_uri_in_activity).await;

    let (key_id, public_key_pem) = match actor_key_result {
        Ok(kp) => kp,
        Err(e) => {
            tracing::warn!(
                "failed to fetch actor key for {}: {}",
                actor_uri_in_activity,
                e
            );
            return api_error(StatusCode::UNAUTHORIZED, "could not retrieve actor public key");
        }
    };

    let path = match target_actor_uri {
        Some(ref uri) => {
            let path_part = uri.strip_prefix(&state.config.base_url).unwrap_or("/inbox");
            format!("{}/inbox", path_part)
        }
        None => "/inbox".to_string(),
    };

    let verify_result = crate::federation::signatures::verify_signature(
        &crate::federation::signatures::IncomingSignature {
            method: "POST",
            path_and_query: &path,
            signature_header: &sig_header,
            request_headers: &req_headers,
            public_key_pem: &public_key_pem,
        },
    );

    if let Err(e) = verify_result {
        tracing::warn!(
            "HTTP Signature verification failed for {} (key {}): {}",
            actor_uri_in_activity,
            key_id,
            e
        );
        return api_error(StatusCode::UNAUTHORIZED, "HTTP Signature verification failed");
    }

    // 6. Deduplication: reject already-processed activity URIs
    if !activity_uri.is_empty() {
        let already_seen: Option<bool> = sqlx::query_scalar(
            "SELECT TRUE FROM federation_activities WHERE activity_uri = $1 LIMIT 1",
        )
        .bind(&activity_uri)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if already_seen.is_some() {
            // Idempotent: already processed, acknowledge without re-processing
            return (StatusCode::ACCEPTED, Json(json!({ "status": "accepted" }))).into_response();
        }
    }

    // 7. Store the inbound activity for async processing
    let activity_id = Uuid::new_v4();
    let object_uri = activity
        .get("object")
        .and_then(|o| {
            if o.is_string() {
                o.as_str().map(|s| s.to_string())
            } else {
                o.get("id").and_then(|id| id.as_str()).map(|s| s.to_string())
            }
        });

    let insert_result = sqlx::query(
        r#"
        INSERT INTO federation_activities (
            id, activity_uri, activity_type, actor_uri, object_uri,
            direction, payload, processing_status, received_at
        ) VALUES (
            $1, $2, $3, $4, $5,
            'inbound', $6, 'pending', NOW()
        )
        "#,
    )
    .bind(activity_id)
    .bind(if activity_uri.is_empty() { None } else { Some(activity_uri.as_str()) })
    .bind(&activity_type)
    .bind(&actor_uri_in_activity)
    .bind(object_uri.as_deref())
    .bind(&activity)
    .execute(&state.pool)
    .await;

    if let Err(e) = insert_result {
        tracing::error!("failed to store inbound activity: {}", e);
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to process activity",
        );
    }

    // Spawn async processing (Follow/Undo) without blocking the response
    let pool_clone = state.pool.clone();
    let actor_clone = actor_uri_in_activity.clone();
    let act_clone = activity.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::federation::activities::handle_inbound_activity(
            &pool_clone,
            &actor_clone,
            &act_clone,
            activity_id,
        )
        .await
        {
            tracing::error!(
                actor_uri = %actor_clone,
                "inbound activity processing error: {}",
                e
            );
        }
    });

    (StatusCode::ACCEPTED, Json(json!({ "status": "accepted" }))).into_response()
}

// ── Utilities ──────────────────────────────────────────────────────────────

fn activitypub_response(body: Value) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/activity+json; charset=utf-8".parse().unwrap(),
    );
    (headers, Json(body)).into_response()
}
