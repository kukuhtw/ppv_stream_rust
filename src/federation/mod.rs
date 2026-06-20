pub mod activities;
pub mod catalog;
pub mod collections;
pub mod delivery;
pub mod keys;
pub mod moderation;
pub mod resolver;
pub mod revenue;
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

        let dev_bypass = std::env::var("FEDERATION_DEV_HTTP_BYPASS")
            .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false);

        if !dev_bypass
            && !self.base_url.starts_with("https://")
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
        // Admin test helper (dev bypass only)
        .route(
            "/api/federation/admin/inject-inbound",
            post(admin_inject_inbound),
        )
        // Admin actor management
        .route(
            "/api/federation/admin/actors/init",
            post(admin_init_actor),
        )
        // Admin outbound follow
        .route(
            "/api/federation/admin/follow",
            post(admin_send_follow),
        )
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
        // Admin — revenue share policies
        .route(
            "/api/federation/admin/revenue/policies",
            get(admin_list_revenue_policies).post(admin_set_revenue_policy),
        )
        // Admin — provider settlement report
        .route(
            "/api/federation/admin/revenue/provider-report",
            get(admin_provider_settlement),
        )
        // Admin — affiliate settlement report
        .route(
            "/api/federation/admin/revenue/affiliate-report",
            get(admin_affiliate_settlement),
        )
        // X402 direct-split referral resolution
        .route(
            "/api/federation/referral/resolve",
            get(resolve_referral_for_split),
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

// ── Admin revenue sharing ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetRevenuePolicyBody {
    domain: String,
    share_basis_points: i32,
}

/// `GET /api/federation/admin/revenue/policies`
async fn admin_list_revenue_policies(
    State(state): State<FederationState>,
    headers: HeaderMap,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    let rows: Vec<(String, i32, bool, String)> = sqlx::query_as(
        "SELECT instance_domain, share_basis_points, is_active, created_at::text \
         FROM revenue_share_policies \
         ORDER BY instance_domain",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let policies: Vec<Value> = rows
        .into_iter()
        .map(|(domain, bp, active, created_at)| {
            json!({
                "instance_domain":    domain,
                "share_basis_points": bp,
                "is_active":          active,
                "created_at":         created_at,
            })
        })
        .collect();

    Json(json!({ "ok": true, "policies": policies })).into_response()
}

/// `POST /api/federation/admin/revenue/policies`
async fn admin_set_revenue_policy(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Json(body): Json<SetRevenuePolicyBody>,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    let domain = body.domain.trim().to_lowercase();
    if domain.is_empty() || domain.contains('/') {
        return api_error(StatusCode::BAD_REQUEST, "domain must be a bare hostname");
    }

    match revenue::set_share_policy(&state.pool, &domain, body.share_basis_points, "admin").await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => {
            tracing::error!("set_share_policy failed: {}", e);
            api_error(StatusCode::BAD_REQUEST, &e.to_string())
        }
    }
}

/// `GET /api/federation/admin/revenue/provider-report`
async fn admin_provider_settlement(
    State(state): State<FederationState>,
    headers: HeaderMap,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    match revenue::provider_settlement_report(&state.pool).await {
        Ok(rows) => Json(json!({ "ok": true, "rows": rows })).into_response(),
        Err(e) => {
            tracing::error!("provider_settlement_report failed: {}", e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "report query failed")
        }
    }
}

/// `GET /api/federation/admin/revenue/affiliate-report`
async fn admin_affiliate_settlement(
    State(state): State<FederationState>,
    headers: HeaderMap,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    match revenue::affiliate_settlement_report(&state.pool).await {
        Ok(rows) => Json(json!({ "ok": true, "rows": rows })).into_response(),
        Err(e) => {
            tracing::error!("affiliate_settlement_report failed: {}", e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "report query failed")
        }
    }
}

/// `GET /api/federation/referral/resolve?token=<token>&actor_url=<actor_url>`
///
/// X402 direct-split support.  The frontend calls this before building the
/// X402 payment to discover whether the referring instance has a wallet
/// address configured and how many basis points they are owed.
///
/// Returns `{ ok, split_enabled, provider_wallet, share_basis_points }`.
/// When `split_enabled` is true the frontend can use a 3-way X402 contract
/// call instead of off-chain accounting.
async fn resolve_referral_for_split(
    State(state): State<FederationState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let token = match params.get("token") {
        Some(t) if !t.is_empty() => t.clone(),
        _ => return api_error(StatusCode::BAD_REQUEST, "token required"),
    };

    let actor_url = match params.get("actor_url") {
        Some(u) if !u.is_empty() => u.clone(),
        _ => return api_error(StatusCode::BAD_REQUEST, "actor_url required"),
    };

    // Fetch the referring actor's public key to verify the token signature
    let key_result = resolver::fetch_remote_actor_key(&actor_url).await;
    let (_key_id, public_key_pem) = match key_result {
        Ok(kp) => kp,
        Err(e) => {
            tracing::warn!("referral resolve: actor key fetch failed: {}", e);
            return api_error(StatusCode::BAD_GATEWAY, "could not retrieve actor public key");
        }
    };

    let claims = match revenue::verify_referral_payload(&token, &public_key_pem) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("referral resolve: invalid token: {}", e);
            return api_error(StatusCode::UNPROCESSABLE_ENTITY, "invalid referral token");
        }
    };

    // Check revenue share policy for this domain
    let policy: Option<(i32, bool)> = sqlx::query_as(
        "SELECT share_basis_points, is_active \
         FROM revenue_share_policies WHERE instance_domain = $1 LIMIT 1",
    )
    .bind(&claims.domain)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (share_bp, is_active) = policy.unwrap_or((0, false));
    let split_enabled = is_active && share_bp > 0;

    // Look up a wallet address for the provider domain (stored in federation_instances)
    let provider_wallet: Option<String> = sqlx::query_scalar(
        "SELECT shared_inbox_url FROM federation_instances WHERE domain = $1 LIMIT 1",
    )
    .bind(&claims.domain)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    // Note: provider_wallet is a placeholder — real implementation requires a
    // dedicated `provider_wallet` column on federation_instances.

    Json(json!({
        "ok": true,
        "referring_domain":  claims.domain,
        "split_enabled":     split_enabled,
        "share_basis_points": if split_enabled { share_bp } else { 0 },
        "provider_wallet":   provider_wallet, // null until wallet column is added
    }))
    .into_response()
}

// ── Admin test helpers (FEDERATION_DEV_HTTP_BYPASS only) ─────────────────

/// `POST /api/federation/admin/inject-inbound`
///
/// Directly process an ActivityPub activity as inbound without HTTP Signature
/// verification.  Only works when `FEDERATION_DEV_HTTP_BYPASS=1`.  Used in
/// integration tests to inject activities from a remote instance without
/// needing to round-trip through the delivery worker.
async fn admin_inject_inbound(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    let bypass = std::env::var("FEDERATION_DEV_HTTP_BYPASS")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);
    if !bypass {
        return api_error(
            StatusCode::FORBIDDEN,
            "inject-inbound requires FEDERATION_DEV_HTTP_BYPASS=1",
        );
    }

    let activity_uri = body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let activity_type = body
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let actor_uri = body
        .get("actor")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    if activity_uri.is_empty() || activity_type.is_empty() || actor_uri.is_empty() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "activity must have id, type, and actor fields",
        );
    }

    // Deduplication: if we already have this activity, return early.
    let existing: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM federation_activities WHERE activity_uri = $1 LIMIT 1",
    )
    .bind(&activity_uri)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if existing.is_some() {
        return Json(json!({
            "ok": true,
            "status": "duplicate",
            "activity_uri": activity_uri
        }))
        .into_response();
    }

    let activity_id = uuid::Uuid::new_v4();
    if let Err(e) = sqlx::query(
        "INSERT INTO federation_activities \
         (id, activity_uri, activity_type, actor_uri, direction, payload, processing_status) \
         VALUES ($1, $2, $3, $4, 'inbound', $5, 'pending')",
    )
    .bind(activity_id)
    .bind(&activity_uri)
    .bind(&activity_type)
    .bind(&actor_uri)
    .bind(&body)
    .execute(&state.pool)
    .await
    {
        tracing::error!("inject-inbound insert failed: {}", e);
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to store activity");
    }

    // Process the activity in the background (mirrors normal inbox processing).
    tokio::spawn({
        let pool = state.pool.clone();
        let actor_uri = actor_uri.clone();
        let body = body.clone();
        async move {
            let _ =
                activities::handle_inbound_activity(&pool, &actor_uri, &body, activity_id).await;
        }
    });

    Json(json!({
        "ok": true,
        "status": "accepted",
        "activity_id": activity_id.to_string(),
        "activity_uri": activity_uri
    }))
    .into_response()
}

// ── Admin actor management ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ActorInitBody {
    username: String,
}

/// `POST /api/federation/admin/actors/init`
///
/// Ensure a local federation actor record and RSA key pair exist for the given
/// `username`.  Idempotent — safe to call multiple times.  Returns the public
/// key PEM.  Intended for integration testing and initial instance setup.
async fn admin_init_actor(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Json(body): Json<ActorInitBody>,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    let username = body.username.trim().to_lowercase();
    if username.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "username required");
    }

    let app_secret: Vec<u8> = std::env::var("HMAC_SECRET")
        .map(|s| s.into_bytes())
        .unwrap_or_default();

    let user_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM users WHERE username = $1 LIMIT 1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let Some(user_id) = user_id else {
        return api_error(StatusCode::NOT_FOUND, "user not found");
    };

    match keys::ensure_local_actor_keys(
        &state.pool,
        &user_id,
        &username,
        &state.config.base_url,
        &state.config.domain,
        &app_secret,
    )
    .await
    {
        Ok(public_key_pem) => {
            let actor_url = format!("{}/users/{}", state.config.base_url, username);
            Json(json!({
                "ok": true,
                "actor_url": actor_url,
                "public_key_pem": public_key_pem
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!("admin_init_actor failed for {}: {}", username, e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "actor init failed")
        }
    }
}

// ── Admin outbound Follow ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct AdminFollowBody {
    local_username: String,
    remote_actor_url: String,
}

/// `POST /api/federation/admin/follow`
///
/// Queue an outbound `Follow` activity from a local actor to a remote actor.
/// The delivery worker will sign and POST it to the remote inbox.
/// Used in integration tests and admin-driven federation setup.
async fn admin_send_follow(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Json(body): Json<AdminFollowBody>,
) -> Response {
    if !moderation::check_admin_token_pub(&headers) {
        return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
    }

    let local_username = body.local_username.trim().to_lowercase();
    let remote_actor_url = body.remote_actor_url.trim().to_string();
    if local_username.is_empty() || remote_actor_url.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "local_username and remote_actor_url required");
    }

    let local_actor_url = format!("{}/users/{}", state.config.base_url, local_username);

    // Verify the local actor exists
    let actor_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM federation_actors WHERE actor_uri = $1 AND is_local = TRUE LIMIT 1",
    )
    .bind(&local_actor_url)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if actor_exists.is_none() {
        return api_error(
            StatusCode::NOT_FOUND,
            "local actor not found — call /admin/actors/init first",
        );
    }

    // Fetch the remote actor to get their inbox URL
    let remote_actor = match activities::upsert_remote_actor(&state.pool, &remote_actor_url).await {
        Ok((_id, inbox)) => inbox,
        Err(e) => {
            tracing::warn!("admin_send_follow: remote actor fetch failed: {}", e);
            return api_error(StatusCode::BAD_GATEWAY, "could not retrieve remote actor");
        }
    };

    let follow_activity_id = uuid::Uuid::new_v4();
    let follow_activity_uri = format!("{}/activities/{}", local_actor_url, follow_activity_id);

    let follow = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": follow_activity_uri,
        "type": "Follow",
        "actor": local_actor_url,
        "object": remote_actor_url,
        "published": chrono::Utc::now().to_rfc3339()
    });

    if let Err(e) = activities::queue_outbound_activity(
        &state.pool,
        &local_actor_url,
        &follow,
        &remote_actor,
    )
    .await
    {
        tracing::error!("admin_send_follow: queue failed: {}", e);
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to queue follow");
    }

    tracing::info!(
        local_actor = %local_actor_url,
        remote_actor = %remote_actor_url,
        "admin Follow queued"
    );

    Json(json!({
        "ok": true,
        "follow_activity_uri": follow_activity_uri,
        "remote_inbox": remote_actor
    }))
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

    // ── Actor JSON serialization tests ────────────────────────────────────

    #[test]
    fn actor_json_has_required_activitypub_fields() {
        let base = "https://example.com";
        let username = "alice";
        let actor_url = format!("{}/users/{}", base, username);

        let actor = serde_json::json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1"
            ],
            "id":              actor_url,
            "type":            "Person",
            "preferredUsername": username,
            "inbox":           format!("{}/inbox", actor_url),
            "outbox":          format!("{}/outbox", actor_url),
            "followers":       format!("{}/followers", actor_url),
            "following":       format!("{}/following", actor_url),
            "endpoints": {
                "sharedInbox": format!("{}/inbox", base)
            }
        });

        assert_eq!(actor["type"], "Person");
        assert_eq!(actor["preferredUsername"], username);
        assert!(actor["inbox"].as_str().unwrap().ends_with("/inbox"));
        assert!(actor["outbox"].as_str().unwrap().ends_with("/outbox"));
        assert!(actor["@context"].is_array());
        let ctx = actor["@context"].as_array().unwrap();
        assert!(ctx.iter().any(|v| v == "https://w3id.org/security/v1"));
    }
}
