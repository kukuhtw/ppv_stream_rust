pub mod activities;
pub mod catalog;
pub mod collections;
pub mod delivery;
pub mod keys;
pub mod moderation;
pub mod resolver;
pub mod signatures;
pub mod video_index;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct FederationConfig {
    pub enabled: bool,
    pub domain: String,
    pub base_url: String,
    pub software_version: String,
}

impl FederationConfig {
    pub fn from_env(default_base_url: &str) -> Self {
        let enabled = parse_bool("FEDERATION_ENABLED", false);
        let base_url = std::env::var("FEDERATION_BASE_URL")
            .unwrap_or_else(|_| default_base_url.trim_end_matches('/').to_string());
        let domain = std::env::var("FEDERATION_DOMAIN").unwrap_or_else(|_| {
            base_url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or("localhost")
                .to_string()
        });

        Self {
            enabled,
            domain,
            base_url: base_url.trim_end_matches('/').to_string(),
            software_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if self.domain.trim().is_empty() {
            return Err("FEDERATION_DOMAIN must not be empty".into());
        }

        if self.domain.contains('/') || self.domain.contains(char::is_whitespace) {
            return Err("FEDERATION_DOMAIN must contain only a host name".into());
        }

        if !self.base_url.starts_with("https://")
            && !self.base_url.starts_with("http://localhost")
        {
            return Err(
                "FEDERATION_BASE_URL must use HTTPS, except for localhost development".into(),
            );
        }

        Ok(())
    }
}

fn parse_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

#[derive(Clone)]
pub struct FederationState {
    pub pool: PgPool,
    pub config: Arc<FederationConfig>,
}

pub fn router(pool: PgPool, default_base_url: &str) -> Result<Router, String> {
    let config = FederationConfig::from_env(default_base_url);
    config.validate()?;

    if !config.enabled {
        tracing::info!("federation disabled");
        return Ok(Router::new());
    }

    tracing::info!(
        domain = %config.domain,
        base_url = %config.base_url,
        "index-only federation enabled"
    );

    let state = FederationState {
        pool,
        config: Arc::new(config),
    };

    Ok(Router::new()
        // Discovery
        .route("/.well-known/webfinger", get(webfinger))
        .route("/.well-known/nodeinfo", get(nodeinfo_well_known))
        .route("/nodeinfo/2.1", get(nodeinfo_21))
        // Actors
        .route("/users/:username", get(local_actor))
        // Collections
        .route(
            "/users/:username/followers",
            get(collections::actor_followers),
        )
        .route(
            "/users/:username/following",
            get(collections::actor_following),
        )
        .route("/users/:username/outbox", get(collections::actor_outbox))
        // Inbox
        .route(
            "/users/:username/inbox",
            get(collections::actor_inbox_get).post(collections::actor_inbox_post),
        )
        .route("/inbox", post(collections::shared_inbox_post))
        // Video index
        .route("/videos/:id", get(catalog::video_ap_object))
        // Federated catalog
        .route("/api/federation/catalog", get(catalog::catalog))
        // Admin moderation — domain rules
        .route(
            "/api/federation/admin/domain-rules",
            get(moderation::list_domain_rules).post(moderation::set_domain_rule),
        )
        .route(
            "/api/federation/admin/domain-rules/:domain",
            axum::routing::delete(moderation::delete_domain_rule),
        )
        // Admin moderation — follow management
        .route(
            "/api/federation/follows/:id/reject",
            axum::routing::post(admin_reject_follow),
        )
        // Admin — overview and known instances
        .route("/api/federation/admin/overview", get(moderation::admin_overview))
        .route("/api/federation/admin/instances", get(moderation::list_instances))
        // Admin — activity log
        .route("/api/federation/admin/activities", get(moderation::list_activities))
        // Admin — delivery queue
        .route("/api/federation/admin/delivery", get(moderation::list_delivery_jobs))
        .route(
            "/api/federation/admin/delivery/:id/retry",
            axum::routing::post(moderation::retry_delivery_job),
        )
        // Admin — remote content removal
        .route(
            "/api/federation/admin/remote-videos/:domain",
            axum::routing::delete(moderation::purge_remote_videos),
        )
        .with_state(state))
}

// ── WebFinger ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct WebFingerQuery {
    resource: String,
}

async fn webfinger(
    State(state): State<FederationState>,
    Query(query): Query<WebFingerQuery>,
) -> Response {
    let Some(handle) = query.resource.strip_prefix("acct:") else {
        return api_error(StatusCode::BAD_REQUEST, "resource must use acct:username@domain");
    };

    let Some((username, domain)) = handle.rsplit_once('@') else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "resource must include username and domain",
        );
    };

    if domain != state.config.domain {
        return api_error(
            StatusCode::NOT_FOUND,
            "account is not hosted by this instance",
        );
    }

    let found = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users \
         WHERE username = $1 AND federation_enabled = TRUE AND discoverable = TRUE",
    )
    .bind(username)
    .fetch_one(&state.pool)
    .await;

    match found {
        Ok(count) if count > 0 => {
            let actor_url = format!("{}/users/{}", state.config.base_url, username);
            Json(json!({
                "subject": format!("acct:{}@{}", username, state.config.domain),
                "aliases": [actor_url],
                "links": [{
                    "rel": "self",
                    "type": "application/activity+json",
                    "href": actor_url
                }]
            }))
            .into_response()
        }
        Ok(_) => api_error(StatusCode::NOT_FOUND, "account not found"),
        Err(error) => {
            tracing::error!(?error, "webfinger user lookup failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database lookup failed")
        }
    }
}

// ── NodeInfo ───────────────────────────────────────────────────────────────

async fn nodeinfo_well_known(State(state): State<FederationState>) -> Json<Value> {
    Json(json!({
        "links": [{
            "rel": "http://nodeinfo.diaspora.software/ns/schema/2.1",
            "href": format!("{}/nodeinfo/2.1", state.config.base_url)
        }]
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeInfoUsageUsers {
    total: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NodeInfoUsage {
    users: NodeInfoUsageUsers,
    local_posts: i64,
}

async fn nodeinfo_21(State(state): State<FederationState>) -> Response {
    let users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool)
        .await;
    let videos = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM videos")
        .fetch_one(&state.pool)
        .await;

    let (Ok(users), Ok(videos)) = (users, videos) else {
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to build node information",
        );
    };

    Json(json!({
        "version": "2.1",
        "software": {
            "name": "ppv_stream_rust",
            "version": state.config.software_version
        },
        "protocols": ["activitypub"],
        "services": {
            "inbound": [],
            "outbound": []
        },
        "openRegistrations": false,
        "usage": NodeInfoUsage {
            users: NodeInfoUsageUsers { total: users },
            local_posts: videos
        },
        "metadata": {
            "federationMode": "index-only",
            "remoteMediaReplication": false,
            "remotePlayback": false
        }
    }))
    .into_response()
}

// ── Local actor ────────────────────────────────────────────────────────────

async fn local_actor(
    State(state): State<FederationState>,
    Path(username): Path<String>,
) -> Response {
    let row: Option<(String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT u.id, u.profile_desc, u.actor_uri, fa.public_key_pem \
             FROM users u \
             LEFT JOIN federation_actors fa \
               ON fa.local_user_id = u.id AND fa.is_local = TRUE \
             WHERE u.username = $1 \
               AND u.federation_enabled = TRUE \
               AND u.discoverable = TRUE \
             LIMIT 1",
        )
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let Some((_user_id, profile_desc, actor_uri, public_key_pem)) = row else {
        return api_error(StatusCode::NOT_FOUND, "actor not found");
    };

    let actor_url =
        actor_uri.unwrap_or_else(|| format!("{}/users/{}", state.config.base_url, username));

    let public_key_block: Value = match public_key_pem {
        Some(pem) => json!({
            "id": format!("{}#main-key", actor_url),
            "owner": actor_url,
            "publicKeyPem": pem
        }),
        None => json!(null),
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/activity+json; charset=utf-8".parse().unwrap(),
    );

    (
        headers,
        Json(json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1"
            ],
            "id": actor_url,
            "type": "Person",
            "preferredUsername": username,
            "name": username,
            "summary": profile_desc.unwrap_or_default(),
            "inbox": format!("{}/inbox", actor_url),
            "outbox": format!("{}/outbox", actor_url),
            "followers": format!("{}/followers", actor_url),
            "following": format!("{}/following", actor_url),
            "url": format!("{}/public/profile.html?username={}", state.config.base_url, username),
            "publicKey": public_key_block,
            "endpoints": {
                "sharedInbox": format!("{}/inbox", state.config.base_url)
            },
            "attachment": [{
                "type": "PropertyValue",
                "name": "Federation mode",
                "value": "Index only. Remote video media is never replicated."
            }]
        })),
    )
        .into_response()
}

// ── Admin moderation ───────────────────────────────────────────────────────

/// `POST /api/federation/follows/:id/reject`
///
/// Admin endpoint: send a `Reject{Follow}` to a remote follower and cancel
/// the follow record.  `id` is the UUID primary key of the
/// `federation_follows` row.
async fn admin_reject_follow(
    State(state): State<FederationState>,
    Path(follow_id): Path<String>,
) -> Response {
    let id = match uuid::Uuid::parse_str(&follow_id) {
        Ok(id) => id,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid follow id"),
    };

    match activities::send_reject(&state.pool, id).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => {
            tracing::error!("admin reject follow {} failed: {}", follow_id, e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "reject failed")
        }
    }
}

// ── Shared helpers ─────────────────────────────────────────────────────────

pub(crate) fn api_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Configuration tests ────────────────────────────────────────────────

    #[test]
    fn disabled_by_default() {
        std::env::remove_var("FEDERATION_ENABLED");
        let config = FederationConfig::from_env("http://localhost:8080");
        assert!(!config.enabled);
    }

    #[test]
    fn rejects_non_https_public_base_url() {
        let config = FederationConfig {
            enabled: true,
            domain: "example.com".into(),
            base_url: "http://example.com".into(),
            software_version: "test".into(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_localhost_http_for_development() {
        let config = FederationConfig {
            enabled: true,
            domain: "localhost:8080".into(),
            base_url: "http://localhost:8080".into(),
            software_version: "test".into(),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_domain_with_slash() {
        let config = FederationConfig {
            enabled: true,
            domain: "example.com/path".into(),
            base_url: "https://example.com".into(),
            software_version: "test".into(),
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_empty_domain() {
        let config = FederationConfig {
            enabled: true,
            domain: "".into(),
            base_url: "https://example.com".into(),
            software_version: "test".into(),
        };
        assert!(config.validate().is_err());
    }

    // ── WebFinger parsing tests ────────────────────────────────────────────

    #[test]
    fn webfinger_resource_must_use_acct_scheme() {
        // The handler splits on "acct:", so non-acct: resources return an error.
        // Simulate the split:
        let resource = "https://example.com/users/alice";
        assert!(resource.strip_prefix("acct:").is_none());
    }

    #[test]
    fn webfinger_handle_requires_at_sign() {
        let handle = "alicenodomain";
        assert!(handle.rsplit_once('@').is_none());
    }

    #[test]
    fn webfinger_valid_handle_splits_correctly() {
        let handle = "alice@example.com";
        let (user, domain) = handle.rsplit_once('@').unwrap();
        assert_eq!(user, "alice");
        assert_eq!(domain, "example.com");
    }

    // ── NodeInfo structure tests ───────────────────────────────────────────

    #[test]
    fn nodeinfo_usage_serialises() {
        let usage = NodeInfoUsage {
            users: NodeInfoUsageUsers { total: 42 },
            local_posts: 7,
        };
        let v = serde_json::to_value(usage).unwrap();
        assert_eq!(v["users"]["total"], 42);
        assert_eq!(v["localPosts"], 7);
    }

    // ── Actor JSON structure tests ─────────────────────────────────────────

    #[test]
    fn actor_url_is_derived_from_base_url_and_username() {
        let base = "https://example.com";
        let username = "alice";
        let actor_url = format!("{}/users/{}", base, username);
        assert_eq!(actor_url, "https://example.com/users/alice");
    }

    #[test]
    fn actor_endpoints_use_canonical_url() {
        let actor_url = "https://example.com/users/alice";
        let inbox = format!("{}/inbox", actor_url);
        let outbox = format!("{}/outbox", actor_url);
        let followers = format!("{}/followers", actor_url);
        let following = format!("{}/following", actor_url);
        assert!(inbox.ends_with("/inbox"));
        assert!(outbox.ends_with("/outbox"));
        assert!(followers.ends_with("/followers"));
        assert!(following.ends_with("/following"));
    }

    #[test]
    fn shared_inbox_is_at_root() {
        let base = "https://example.com";
        let shared_inbox = format!("{}/inbox", base);
        assert_eq!(shared_inbox, "https://example.com/inbox");
    }
}
