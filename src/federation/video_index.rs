use anyhow::Context;
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

/// The ActivityPub `as:Public` audience URI.
const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

// ── AP object building ─────────────────────────────────────────────────────

/// Build the ActivityPub Video object for a local video.
///
/// Returns `None` when the video is not eligible for federation
/// (e.g., `federation_visibility` is not `public`).
pub async fn build_video_object(
    pool: &PgPool,
    video_id: &str,
    base_url: &str,
) -> anyhow::Result<Option<Value>> {
    let row: Option<(
        String,         // title
        String,         // description
        i64,            // price_cents
        String,         // owner_id
        String,         // federation_visibility
        Option<String>, // object_uri
    )> = sqlx::query_as(
        "SELECT title, description, price_cents, owner_id, \
                federation_visibility, object_uri \
         FROM videos WHERE id = $1 LIMIT 1",
    )
    .bind(video_id)
    .fetch_optional(pool)
    .await
    .context("video lookup failed")?;

    let Some((title, description, price_cents, owner_id, visibility, existing_object_uri)) = row
    else {
        return Ok(None);
    };

    if visibility != "public" {
        return Ok(None);
    }

    // Resolve the actor URI for the owner
    let actor_uri: Option<String> =
        sqlx::query_scalar("SELECT actor_uri FROM users WHERE id = $1 LIMIT 1")
            .bind(&owner_id)
            .fetch_optional(pool)
            .await
            .context("owner actor URI lookup failed")?
            .flatten();

    let actor_uri = actor_uri.unwrap_or_else(|| format!("{}/users/{}", base_url, owner_id));

    // Assign or reuse the object URI
    let object_uri =
        existing_object_uri.unwrap_or_else(|| format!("{}/videos/{}", base_url, video_id));

    // Price in major units (e.g. cents → dollars)
    let price_major = (price_cents as f64) / 100.0;
    let price_str = format!("{:.2}", price_major);

    let checkout_url = format!("{}/checkout/{}", base_url, video_id);
    let watch_url = format!("{}/watch/{}", base_url, video_id);

    Ok(Some(json!({
        "@context": [
            "https://www.w3.org/ns/activitystreams",
            {
                "PPVStream":    "https://ppvstream.org/ns/1.0#",
                "priceAmount":  "PPVStream:priceAmount",
                "priceCurrency": "PPVStream:priceCurrency",
                "checkoutUrl":  "PPVStream:checkoutUrl"
            }
        ],
        "id":          object_uri,
        "type":        "Video",
        "name":        title,
        "content":     description,
        "attributedTo": actor_uri,
        "to":          [AP_PUBLIC],
        "published":   Utc::now().to_rfc3339(),
        "url": [{
            "type":      "Link",
            "href":      watch_url,
            "mediaType": "text/html",
            "name":      "Watch on origin"
        }],
        "priceAmount":  price_str,
        "priceCurrency": "USD",
        "checkoutUrl":  checkout_url
    })))
}

// ── Publishing outbound activities ─────────────────────────────────────────

/// Assign `object_uri` to a local video, then broadcast a `Create` activity
/// to all followers of the video owner.
///
/// Returns the assigned object URI, or `None` when the video is not eligible.
#[allow(dead_code)]
pub async fn publish_create(
    pool: &PgPool,
    video_id: &str,
    base_url: &str,
) -> anyhow::Result<Option<String>> {
    // Ensure object_uri is set
    let object_uri = format!("{}/videos/{}", base_url, video_id);
    sqlx::query(
        "UPDATE videos SET object_uri = $2, federated_at = NOW(), \
                          federation_updated_at = NOW() \
         WHERE id = $1 AND object_uri IS NULL",
    )
    .bind(video_id)
    .bind(&object_uri)
    .execute(pool)
    .await
    .context("video object_uri assignment failed")?;

    let video_obj = build_video_object(pool, video_id, base_url).await?;
    let Some(video_obj) = video_obj else {
        return Ok(None);
    };

    let actor_uri = video_obj["attributedTo"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let activity_id = format!("{}/activities/{}", base_url, Uuid::new_v4());
    let activity = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id":     activity_id,
        "type":   "Create",
        "actor":  actor_uri,
        "to":     [AP_PUBLIC],
        "object": video_obj
    });

    broadcast_to_followers(pool, &actor_uri, &activity, base_url).await?;
    Ok(Some(object_uri))
}

/// Broadcast an `Update` for an already-federated local video.
#[allow(dead_code)]
pub async fn publish_update(pool: &PgPool, video_id: &str, base_url: &str) -> anyhow::Result<()> {
    let video_obj = build_video_object(pool, video_id, base_url).await?;
    let Some(video_obj) = video_obj else {
        return Ok(());
    };

    let actor_uri = video_obj["attributedTo"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    sqlx::query("UPDATE videos SET federation_updated_at = NOW() WHERE id = $1")
        .bind(video_id)
        .execute(pool)
        .await
        .context("video federation_updated_at refresh failed")?;

    let activity = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id":     format!("{}/activities/{}", base_url, Uuid::new_v4()),
        "type":   "Update",
        "actor":  actor_uri,
        "to":     [AP_PUBLIC],
        "object": video_obj
    });

    broadcast_to_followers(pool, &actor_uri, &activity, base_url).await
}

/// Broadcast a `Delete` for a local video that is being removed or hidden.
#[allow(dead_code)]
pub async fn publish_delete(pool: &PgPool, video_id: &str, base_url: &str) -> anyhow::Result<()> {
    // Look up the owner and object_uri before the video is removed
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT owner_id, object_uri FROM videos WHERE id = $1 LIMIT 1")
            .bind(video_id)
            .fetch_optional(pool)
            .await
            .context("video lookup for Delete failed")?;

    let Some((owner_id, Some(object_uri))) = row else {
        return Ok(()); // Video was never federated
    };

    let actor_uri: Option<String> =
        sqlx::query_scalar("SELECT actor_uri FROM users WHERE id = $1 LIMIT 1")
            .bind(&owner_id)
            .fetch_optional(pool)
            .await
            .context("owner actor lookup failed")?
            .flatten();

    let actor_uri = actor_uri.unwrap_or_else(|| format!("{}/users/{}", base_url, owner_id));

    let activity = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id":     format!("{}/activities/{}", base_url, Uuid::new_v4()),
        "type":   "Delete",
        "actor":  actor_uri,
        "to":     [AP_PUBLIC],
        "object": object_uri
    });

    broadcast_to_followers(pool, &actor_uri, &activity, base_url).await
}

// ── Processing inbound remote video activities ─────────────────────────────

/// Process a remote `Create{Video}` activity.
pub async fn process_remote_create(
    pool: &PgPool,
    actor_uri: &str,
    object: &Value,
) -> anyhow::Result<()> {
    upsert_remote_video(pool, actor_uri, object, false).await
}

/// Process a remote `Update{Video}` activity.
pub async fn process_remote_update(
    pool: &PgPool,
    actor_uri: &str,
    object: &Value,
) -> anyhow::Result<()> {
    upsert_remote_video(pool, actor_uri, object, false).await
}

/// Process a remote `Delete` activity (object may be a URI string or object).
pub async fn process_remote_delete(pool: &PgPool, object_uri: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE remote_video_catalog \
         SET is_deleted = TRUE, availability_status = 'deleted', updated_at = NOW() \
         WHERE object_uri = $1",
    )
    .bind(object_uri)
    .execute(pool)
    .await
    .context("remote video delete failed")?;
    Ok(())
}

fn extract_object_uri(object: &Value) -> Option<&str> {
    if object.is_string() {
        object.as_str()
    } else {
        object.get("id")?.as_str()
    }
}

fn is_video_object(object: &Value) -> bool {
    object
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "Video")
        .unwrap_or(false)
}

/// Returns the object and its URI when the activity has a Video object.
pub fn extract_video_object(activity: &Value) -> Option<(&Value, &str)> {
    let obj = activity.get("object")?;
    if !is_video_object(obj) {
        return None;
    }
    let uri = extract_object_uri(obj)?;
    Some((obj, uri))
}

async fn upsert_remote_video(
    pool: &PgPool,
    actor_uri: &str,
    object: &Value,
    mark_deleted: bool,
) -> anyhow::Result<()> {
    let object_uri = object
        .get("id")
        .and_then(|id| id.as_str())
        .ok_or_else(|| anyhow::anyhow!("Video object has no id"))?;

    if !is_video_object(object) {
        return Ok(());
    }

    let origin_domain = actor_uri
        .strip_prefix("https://")
        .or_else(|| actor_uri.strip_prefix("http://"))
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("unknown");

    let title = object
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Untitled")
        .to_string();

    let description = object
        .get("content")
        .or_else(|| object.get("summary"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

    let canonical_url = object
        .get("url")
        .and_then(|u| {
            if u.is_string() {
                u.as_str().map(|s| s.to_string())
            } else if u.is_array() {
                u.as_array()?
                    .iter()
                    .find(|item| {
                        item.get("mediaType")
                            .and_then(|m| m.as_str())
                            .map(|m| m == "text/html")
                            .unwrap_or(false)
                    })
                    .and_then(|item| item.get("href"))
                    .and_then(|h| h.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| object_uri.to_string());

    let checkout_url = object
        .get("checkoutUrl")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());

    let price_amount = object
        .get("priceAmount")
        .and_then(|p| p.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let price_currency = object
        .get("priceCurrency")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    let availability_status = if mark_deleted { "deleted" } else { "available" };

    let published_at = object
        .get("published")
        .and_then(|p| p.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    sqlx::query(
        r#"
        INSERT INTO remote_video_catalog (
            id, object_uri, origin_actor_uri, origin_domain,
            title, description, canonical_url, checkout_url,
            price_amount, price_currency,
            availability_status, published_at,
            raw_object, is_deleted, fetched_at, updated_at
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7, $8,
            $9, $10,
            $11, $12,
            $13, $14, NOW(), NOW()
        )
        ON CONFLICT (object_uri) DO UPDATE SET
            origin_actor_uri    = EXCLUDED.origin_actor_uri,
            title               = EXCLUDED.title,
            description         = EXCLUDED.description,
            canonical_url       = EXCLUDED.canonical_url,
            checkout_url        = EXCLUDED.checkout_url,
            price_amount        = EXCLUDED.price_amount,
            price_currency      = EXCLUDED.price_currency,
            availability_status = EXCLUDED.availability_status,
            published_at        = EXCLUDED.published_at,
            raw_object          = EXCLUDED.raw_object,
            is_deleted          = EXCLUDED.is_deleted,
            updated_at          = NOW()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(object_uri)
    .bind(actor_uri)
    .bind(origin_domain)
    .bind(&title)
    .bind(description.as_deref())
    .bind(&canonical_url)
    .bind(checkout_url.as_deref())
    .bind(price_amount)
    .bind(price_currency.as_deref())
    .bind(availability_status)
    .bind(published_at)
    .bind(object)
    .bind(mark_deleted)
    .execute(pool)
    .await
    .context("remote video catalog upsert failed")?;

    Ok(())
}

// ── Follower distribution ──────────────────────────────────────────────────

async fn broadcast_to_followers(
    pool: &PgPool,
    actor_uri: &str,
    activity: &Value,
    _base_url: &str,
) -> anyhow::Result<()> {
    let inboxes = follower_delivery_inboxes(pool, actor_uri).await?;

    if inboxes.is_empty() {
        return Ok(());
    }

    for inbox_url in inboxes {
        crate::federation::activities::queue_outbound_activity(
            pool, actor_uri, activity, &inbox_url,
        )
        .await
        .context("queuing broadcast delivery failed")?;
    }

    Ok(())
}

/// Returns deduplicated delivery URLs for all accepted followers of `actor_uri`.
/// Uses shared inbox where available to reduce delivery volume.
async fn follower_delivery_inboxes(pool: &PgPool, actor_uri: &str) -> anyhow::Result<Vec<String>> {
    let rows: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT fa.shared_inbox_url, fa.inbox_url \
         FROM federation_follows ff \
         JOIN federation_actors local ON local.id = ff.following_actor_id \
         JOIN federation_actors fa    ON fa.id    = ff.follower_actor_id \
         WHERE local.actor_uri = $1 AND ff.status = 'accepted'",
    )
    .bind(actor_uri)
    .fetch_all(pool)
    .await
    .context("follower inbox query failed")?;

    // Prefer shared_inbox; deduplicate
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for (shared, personal) in rows {
        let url = shared.unwrap_or(personal);
        if seen.insert(url.clone()) {
            result.push(url);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_video_object_correct() {
        assert!(is_video_object(
            &json!({"type": "Video", "id": "https://ex.com/v/1"})
        ));
        assert!(!is_video_object(&json!({"type": "Note"})));
        assert!(!is_video_object(&json!({})));
    }

    #[test]
    fn extract_video_object_from_create() {
        let activity = json!({
            "type": "Create",
            "object": {
                "id": "https://ex.com/videos/1",
                "type": "Video",
                "name": "Test"
            }
        });
        let result = extract_video_object(&activity);
        assert!(result.is_some());
        let (obj, uri) = result.unwrap();
        assert_eq!(uri, "https://ex.com/videos/1");
        assert_eq!(obj["name"], "Test");
    }

    #[test]
    fn extract_video_object_from_non_video_returns_none() {
        let activity = json!({
            "type": "Create",
            "object": { "id": "https://ex.com/notes/1", "type": "Note" }
        });
        assert!(extract_video_object(&activity).is_none());
    }

    #[test]
    fn canonical_url_prefers_html_link() {
        // Simulates how we parse the url array
        let urls = json!([
            {"type": "Link", "href": "https://ex.com/hls/video.m3u8", "mediaType": "application/x-mpegURL"},
            {"type": "Link", "href": "https://ex.com/watch/1", "mediaType": "text/html"}
        ]);
        let html = urls
            .as_array()
            .unwrap()
            .iter()
            .find(|item| {
                item.get("mediaType")
                    .and_then(|m| m.as_str())
                    .map(|m| m == "text/html")
                    .unwrap_or(false)
            })
            .and_then(|item| item.get("href"))
            .and_then(|h| h.as_str());
        assert_eq!(html, Some("https://ex.com/watch/1"));
    }
}
