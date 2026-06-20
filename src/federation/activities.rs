use anyhow::Context;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

/// Dispatch inbound activity processing by type.
///
/// Called asynchronously after an inbound activity has been stored in
/// `federation_activities` with status `pending`.
pub async fn handle_inbound_activity(
    pool: &PgPool,
    actor_uri: &str,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    let activity_type = activity
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let result = match activity_type {
        "Follow" => handle_follow(pool, actor_uri, activity, activity_db_id).await,
        "Accept" => handle_accept(pool, actor_uri, activity, activity_db_id).await,
        "Reject" => handle_reject(pool, actor_uri, activity, activity_db_id).await,
        "Undo" => handle_undo(pool, actor_uri, activity, activity_db_id).await,
        "Create" => handle_create_or_update(pool, actor_uri, activity, activity_db_id).await,
        "Update" => handle_create_or_update(pool, actor_uri, activity, activity_db_id).await,
        "Delete" => handle_delete(pool, activity, activity_db_id).await,
        _ => mark_activity(pool, activity_db_id, "ignored").await,
    };

    if let Err(ref e) = result {
        tracing::warn!(
            %actor_uri,
            activity_type,
            activity_id = %activity_db_id,
            "activity processing failed: {}",
            e
        );
        mark_activity(pool, activity_db_id, "failed").await.ok();
    }

    result
}

// ── Follow ─────────────────────────────────────────────────────────────────

async fn handle_follow(
    pool: &PgPool,
    follower_uri: &str,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    // Extract the object — the local actor URI being followed
    let object_uri = extract_object_id(activity)
        .ok_or_else(|| anyhow::anyhow!("Follow activity has no recognisable object"))?;

    let follow_activity_uri = activity
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("");

    // Find local actor record
    let local: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, actor_uri FROM federation_actors \
         WHERE actor_uri = $1 AND is_local = TRUE LIMIT 1",
    )
    .bind(object_uri)
    .fetch_optional(pool)
    .await
    .context("local actor lookup failed")?;

    let Some((local_db_id, local_actor_uri)) = local else {
        // Follow targets an actor not managed by this instance — ignore
        return mark_activity(pool, activity_db_id, "ignored").await;
    };

    // Upsert remote actor (fetch from network if not cached)
    let (follower_db_id, follower_inbox) =
        upsert_remote_actor(pool, follower_uri).await?;

    // Record the follow as accepted (this instance auto-accepts all follows)
    sqlx::query(
        r#"
        INSERT INTO federation_follows (
            id, follower_actor_id, following_actor_id, activity_uri, status
        ) VALUES ($1, $2, $3, $4, 'accepted')
        ON CONFLICT (follower_actor_id, following_actor_id)
            DO UPDATE SET
                status       = 'accepted',
                activity_uri = EXCLUDED.activity_uri,
                updated_at   = NOW()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(follower_db_id)
    .bind(local_db_id)
    .bind(follow_activity_uri)
    .execute(pool)
    .await
    .context("federation_follows upsert failed")?;

    mark_activity(pool, activity_db_id, "processed").await?;

    // Queue an Accept{Follow} back to the follower's inbox
    let accept = build_accept(&local_actor_uri, activity);
    queue_outbound_activity(pool, &local_actor_uri, &accept, &follower_inbox).await
        .context("queuing Accept{Follow} failed")?;

    tracing::info!(
        follower_uri,
        local_actor_uri = %local_actor_uri,
        "Follow accepted"
    );
    Ok(())
}

// ── Accept ─────────────────────────────────────────────────────────────────

/// Handle an incoming `Accept{Follow}` from a remote actor — they approved a
/// Follow that this instance sent.  Currently a no-op placeholder: we don't
/// send outbound Follows yet, but when we do the accepted follow record will
/// need to be updated here.
async fn handle_accept(
    pool: &PgPool,
    _actor_uri: &str,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    // The object should be the original Follow activity URI we sent.
    let follow_uri = extract_object_id(activity).unwrap_or("");

    if !follow_uri.is_empty() {
        sqlx::query(
            "UPDATE federation_follows ff SET status = 'accepted', updated_at = NOW()
             FROM federation_actors fa
             WHERE fa.id = ff.following_actor_id
               AND fa.is_local = TRUE
               AND ff.activity_uri = $1",
        )
        .bind(follow_uri)
        .execute(pool)
        .await
        .context("Accept{Follow} status update failed")?;
    }

    mark_activity(pool, activity_db_id, "processed").await
}

// ── Reject ─────────────────────────────────────────────────────────────────

/// Handle an incoming `Reject{Follow}` from a remote actor — they declined a
/// Follow that this instance sent.  Marks the follow record as `rejected`.
async fn handle_reject(
    pool: &PgPool,
    _actor_uri: &str,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    let follow_uri = extract_object_id(activity).unwrap_or("");

    if !follow_uri.is_empty() {
        sqlx::query(
            "UPDATE federation_follows ff SET status = 'rejected', updated_at = NOW()
             FROM federation_actors fa
             WHERE fa.id = ff.following_actor_id
               AND fa.is_local = TRUE
               AND ff.activity_uri = $1",
        )
        .bind(follow_uri)
        .execute(pool)
        .await
        .context("Reject{Follow} status update failed")?;
    }

    mark_activity(pool, activity_db_id, "processed").await
}

/// Send a `Reject{Follow}` to a remote follower and cancel the follow record.
///
/// Called from the admin API when a moderator explicitly rejects an incoming
/// follow request.  `follow_db_id` is the UUID primary key of the
/// `federation_follows` row.
pub async fn send_reject(
    pool: &PgPool,
    follow_db_id: Uuid,
) -> anyhow::Result<()> {
    // Load the follow row: follower inbox + local actor URI + follow activity URI
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        r#"
        SELECT
            follower.actor_uri,
            follower.inbox_url,
            local_actor.actor_uri,
            ff.activity_uri
        FROM federation_follows ff
        JOIN federation_actors follower   ON follower.id   = ff.follower_actor_id
        JOIN federation_actors local_actor ON local_actor.id = ff.following_actor_id
        WHERE ff.id = $1 AND local_actor.is_local = TRUE
        LIMIT 1
        "#,
    )
    .bind(follow_db_id)
    .fetch_optional(pool)
    .await
    .context("follow lookup for Reject failed")?;

    let Some((follower_uri, follower_inbox, local_actor_uri, follow_activity_uri)) = row else {
        anyhow::bail!("follow record {} not found or not targeting a local actor", follow_db_id);
    };

    // Mark the follow as rejected
    sqlx::query(
        "UPDATE federation_follows SET status = 'rejected', updated_at = NOW() WHERE id = $1",
    )
    .bind(follow_db_id)
    .execute(pool)
    .await
    .context("follow status → rejected failed")?;

    // Build and queue the Reject activity
    let reject = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/{}", local_actor_uri, Uuid::new_v4()),
        "type": "Reject",
        "actor": local_actor_uri,
        "object": {
            "type": "Follow",
            "id":   follow_activity_uri,
            "actor": follower_uri,
            "object": local_actor_uri
        },
        "published": chrono::Utc::now().to_rfc3339()
    });

    queue_outbound_activity(pool, &local_actor_uri, &reject, &follower_inbox)
        .await
        .context("queuing Reject{Follow} failed")?;

    tracing::info!(
        %follower_uri,
        %local_actor_uri,
        "Reject{{Follow}} queued"
    );
    Ok(())
}

// ── Undo ───────────────────────────────────────────────────────────────────

async fn handle_undo(
    pool: &PgPool,
    actor_uri: &str,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    let obj = activity.get("object");
    let obj_type = obj
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("");

    if obj_type != "Follow" {
        return mark_activity(pool, activity_db_id, "ignored").await;
    }

    let follow_uri = obj
        .and_then(|o| o.get("id"))
        .and_then(|id| id.as_str())
        .or_else(|| obj.and_then(|o| o.as_str()))
        .unwrap_or("");

    // Cancel the follow record for this actor + activity_uri combination
    sqlx::query(
        "UPDATE federation_follows ff SET status = 'cancelled', updated_at = NOW()
         FROM federation_actors fa
         WHERE fa.id = ff.follower_actor_id
           AND fa.actor_uri = $1
           AND ff.activity_uri = $2",
    )
    .bind(actor_uri)
    .bind(follow_uri)
    .execute(pool)
    .await
    .context("Undo{Follow} update failed")?;

    mark_activity(pool, activity_db_id, "processed").await?;

    tracing::info!(
        actor_uri,
        follow_uri,
        "Undo{{Follow}} processed"
    );
    Ok(())
}

// ── Accept / Reject ────────────────────────────────────────────────────────
// (These arrive when a remote instance responds to our own Follow requests,
// which is future work in Phase 4 / Phase 5.)

// ── Create / Update ────────────────────────────────────────────────────────

async fn handle_create_or_update(
    pool: &PgPool,
    actor_uri: &str,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    use crate::federation::video_index::extract_video_object;

    if let Some((obj, _uri)) = extract_video_object(activity) {
        let activity_type = activity
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("Create");

        let result = if activity_type == "Update" {
            crate::federation::video_index::process_remote_update(pool, actor_uri, obj).await
        } else {
            crate::federation::video_index::process_remote_create(pool, actor_uri, obj).await
        };

        result.context("remote video catalog update failed")?;
        mark_activity(pool, activity_db_id, "processed").await
    } else {
        // Non-video Create/Update (e.g. Note) — not relevant to this index
        mark_activity(pool, activity_db_id, "ignored").await
    }
}

// ── Delete ─────────────────────────────────────────────────────────────────

async fn handle_delete(
    pool: &PgPool,
    activity: &Value,
    activity_db_id: Uuid,
) -> anyhow::Result<()> {
    let object_uri = activity
        .get("object")
        .and_then(|o| {
            if o.is_string() {
                o.as_str()
            } else {
                o.get("id")?.as_str()
            }
        });

    if let Some(uri) = object_uri {
        crate::federation::video_index::process_remote_delete(pool, uri)
            .await
            .context("remote video delete failed")?;
    }

    mark_activity(pool, activity_db_id, "processed").await
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn extract_object_id(activity: &Value) -> Option<&str> {
    let obj = activity.get("object")?;
    if obj.is_string() {
        obj.as_str()
    } else {
        obj.get("id")?.as_str()
    }
}

fn build_accept(local_actor_uri: &str, follow_activity: &Value) -> Value {
    json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/activities/{}", local_actor_uri, Uuid::new_v4()),
        "type": "Accept",
        "actor": local_actor_uri,
        "object": follow_activity,
        "published": Utc::now().to_rfc3339()
    })
}

/// Insert or update a remote actor record.
///
/// Fetches the actor document from the network if no cached record exists.
/// Returns `(actor_db_id, inbox_url)`.
pub async fn upsert_remote_actor(
    pool: &PgPool,
    actor_uri: &str,
) -> anyhow::Result<(Uuid, String)> {
    // Fast path: record already in DB
    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, inbox_url FROM federation_actors \
         WHERE actor_uri = $1 AND is_local = FALSE LIMIT 1",
    )
    .bind(actor_uri)
    .fetch_optional(pool)
    .await
    .context("remote actor cache lookup failed")?;

    if let Some(row) = existing {
        return Ok(row);
    }

    // Slow path: fetch actor document from network
    let doc = crate::federation::resolver::fetch_remote_object(actor_uri)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch remote actor {}: {}", actor_uri, e))?;

    let inbox_url = doc
        .get("inbox")
        .and_then(|u| u.as_str())
        .ok_or_else(|| anyhow::anyhow!("remote actor {} has no inbox URL", actor_uri))?
        .to_string();

    let username = doc
        .get("preferredUsername")
        .and_then(|u| u.as_str())
        .or_else(|| actor_uri.rsplit('/').next())
        .unwrap_or("unknown")
        .to_string();

    let domain = actor_uri
        .strip_prefix("https://")
        .or_else(|| actor_uri.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("unknown")
        .to_string();

    let public_key_id = doc
        .get("publicKey")
        .and_then(|pk| pk.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string());

    let public_key_pem = doc
        .get("publicKey")
        .and_then(|pk| pk.get("publicKeyPem"))
        .and_then(|pem| pem.as_str())
        .map(|s| s.to_string());

    let shared_inbox = doc
        .get("endpoints")
        .and_then(|ep| ep.get("sharedInbox"))
        .and_then(|si| si.as_str())
        .map(|s| s.to_string());

    let returned_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO federation_actors (
            id, actor_uri, username, domain,
            inbox_url, shared_inbox_url,
            public_key_id, public_key_pem,
            is_local, fetched_at
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6,
            $7, $8,
            FALSE, NOW()
        )
        ON CONFLICT (actor_uri) DO UPDATE SET
            inbox_url        = EXCLUDED.inbox_url,
            shared_inbox_url = EXCLUDED.shared_inbox_url,
            public_key_id    = EXCLUDED.public_key_id,
            public_key_pem   = EXCLUDED.public_key_pem,
            fetched_at       = NOW(),
            updated_at       = NOW()
        RETURNING id
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(actor_uri)
    .bind(&username)
    .bind(&domain)
    .bind(&inbox_url)
    .bind(shared_inbox.as_deref())
    .bind(public_key_id.as_deref())
    .bind(public_key_pem.as_deref())
    .fetch_one(pool)
    .await
    .context("remote actor upsert failed")?;

    Ok((returned_id, inbox_url))
}

/// Insert an outbound activity record and schedule a delivery job.
pub async fn queue_outbound_activity(
    pool: &PgPool,
    actor_uri: &str,
    payload: &Value,
    target_inbox_url: &str,
) -> anyhow::Result<()> {
    let activity_uri = payload
        .get("id")
        .and_then(|id| id.as_str())
        .filter(|s| !s.is_empty());
    let activity_type = payload
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("Unknown");

    let activity_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO federation_activities (
            id, activity_uri, activity_type, actor_uri,
            direction, payload, processing_status
        ) VALUES ($1, $2, $3, $4, 'outbound', $5, 'pending')
        "#,
    )
    .bind(activity_id)
    .bind(activity_uri)
    .bind(activity_type)
    .bind(actor_uri)
    .bind(payload)
    .execute(pool)
    .await
    .context("outbound activity insert failed")?;

    sqlx::query(
        r#"
        INSERT INTO federation_delivery_jobs (
            id, activity_id, target_inbox_url, next_attempt_at
        ) VALUES ($1, $2, $3, NOW())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(activity_id)
    .bind(target_inbox_url)
    .execute(pool)
    .await
    .context("delivery job insert failed")?;

    Ok(())
}

async fn mark_activity(
    pool: &PgPool,
    id: Uuid,
    status: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE federation_activities \
         SET processing_status = $2, processed_at = NOW() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .execute(pool)
    .await
    .context("activity status update failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_accept_contains_required_fields() {
        let follow = json!({
            "id": "https://remote.example/activities/1",
            "type": "Follow",
            "actor": "https://remote.example/users/bob",
            "object": "https://local.example/users/alice"
        });
        let accept = build_accept("https://local.example/users/alice", &follow);
        assert_eq!(accept["type"], "Accept");
        assert_eq!(accept["actor"], "https://local.example/users/alice");
        assert!(accept["id"]
            .as_str()
            .unwrap()
            .starts_with("https://local.example/users/alice/activities/"));
    }

    #[test]
    fn extract_object_id_from_string() {
        let activity = json!({ "object": "https://example.com/users/alice" });
        assert_eq!(
            extract_object_id(&activity),
            Some("https://example.com/users/alice")
        );
    }

    #[test]
    fn extract_object_id_from_object() {
        let activity = json!({ "object": { "id": "https://example.com/users/alice", "type": "Person" } });
        assert_eq!(
            extract_object_id(&activity),
            Some("https://example.com/users/alice")
        );
    }

    #[test]
    fn extract_object_id_missing_returns_none() {
        let activity = json!({ "type": "Follow" });
        assert!(extract_object_id(&activity).is_none());
    }

    #[test]
    fn reject_activity_has_required_fields() {
        let local_actor = "https://local.example/users/alice";
        let follow_uri  = "https://remote.example/activities/f1";
        let follower    = "https://remote.example/users/bob";

        let reject = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id":     format!("{}/activities/{}", local_actor, uuid::Uuid::new_v4()),
            "type":   "Reject",
            "actor":  local_actor,
            "object": {
                "type":   "Follow",
                "id":     follow_uri,
                "actor":  follower,
                "object": local_actor
            }
        });

        assert_eq!(reject["type"], "Reject");
        assert_eq!(reject["actor"], local_actor);
        assert_eq!(reject["object"]["type"], "Follow");
        assert_eq!(reject["object"]["id"], follow_uri);
    }
}
