use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::{api_error, FederationState};

// ── Admin token auth ───────────────────────────────────────────────────────

/// Checks `X-Federation-Admin-Token` header against `FEDERATION_ADMIN_TOKEN` env var.
pub fn check_admin_token_pub(headers: &HeaderMap) -> bool {
    check_admin_token(headers)
}

fn check_admin_token(headers: &HeaderMap) -> bool {
    let expected = match std::env::var("FEDERATION_ADMIN_TOKEN") {
        Ok(t) if !t.trim().is_empty() => t,
        _ => return false,
    };
    headers
        .get("x-federation-admin-token")
        .and_then(|v| v.to_str().ok())
        .map(|t| t == expected.trim())
        .unwrap_or(false)
}

macro_rules! require_admin {
    ($headers:expr) => {
        if !check_admin_token(&$headers) {
            return api_error(StatusCode::FORBIDDEN, "X-Federation-Admin-Token required");
        }
    };
}

// ── Domain rule enforcement ────────────────────────────────────────────────

/// Returns the configured action for `domain`, or `None` if no rule exists.
///
/// Callers use this to enforce `block`, `suspend`, `silence`, `reject_media`,
/// or `allow` rules before processing inbound activities or dispatching
/// outbound deliveries.
pub async fn domain_action(pool: &PgPool, domain: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT action FROM federation_domain_rules WHERE domain = $1 LIMIT 1",
    )
    .bind(domain)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

/// Returns `true` when the domain has an active `block` or `suspend` rule.
pub async fn is_domain_blocked(pool: &PgPool, domain: &str) -> bool {
    matches!(
        domain_action(pool, domain).await.as_deref(),
        Some("block") | Some("suspend")
    )
}

// ── Shared query helpers ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

// ── Domain rule endpoints ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetDomainRuleBody {
    pub domain: String,
    pub action: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// `GET /api/federation/admin/domain-rules`
pub async fn list_domain_rules(
    State(state): State<FederationState>,
    headers: HeaderMap,
) -> Response {
    require_admin!(headers);

    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT domain, action, reason, created_at::text \
         FROM federation_domain_rules \
         ORDER BY action, domain",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let rules: Vec<Value> = rows
        .into_iter()
        .map(|(domain, action, reason, created_at)| {
            json!({ "domain": domain, "action": action, "reason": reason, "created_at": created_at })
        })
        .collect();

    Json(json!({ "ok": true, "rules": rules })).into_response()
}

/// `POST /api/federation/admin/domain-rules`
///
/// Insert or update a domain moderation rule.
/// `action` must be one of: `allow`, `silence`, `reject_media`, `suspend`, `block`.
pub async fn set_domain_rule(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Json(body): Json<SetDomainRuleBody>,
) -> Response {
    require_admin!(headers);

    const VALID: &[&str] = &["allow", "silence", "reject_media", "suspend", "block"];
    if !VALID.contains(&body.action.as_str()) {
        return api_error(
            StatusCode::BAD_REQUEST,
            "action must be: allow, silence, reject_media, suspend, or block",
        );
    }

    let domain = body.domain.trim().to_lowercase();
    if domain.is_empty() || domain.contains('/') || domain.contains('@') {
        return api_error(StatusCode::BAD_REQUEST, "domain must be a bare hostname");
    }

    let result = sqlx::query(
        r#"
        INSERT INTO federation_domain_rules (id, domain, action, reason, created_by)
        VALUES ($1, $2, $3, $4, 'admin')
        ON CONFLICT (domain) DO UPDATE
            SET action     = EXCLUDED.action,
                reason     = EXCLUDED.reason,
                updated_at = NOW()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&domain)
    .bind(&body.action)
    .bind(&body.reason)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => {
            tracing::info!(domain, action = %body.action, "domain rule set");
            Json(json!({ "ok": true })).into_response()
        }
        Err(e) => {
            tracing::error!("set_domain_rule failed: {}", e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

/// `DELETE /api/federation/admin/domain-rules/:domain`
pub async fn delete_domain_rule(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Path(domain): Path<String>,
) -> Response {
    require_admin!(headers);

    let result = sqlx::query("DELETE FROM federation_domain_rules WHERE domain = $1")
        .bind(domain.trim().to_lowercase())
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(domain, "domain rule removed");
            Json(json!({ "ok": true })).into_response()
        }
        Ok(_) => api_error(StatusCode::NOT_FOUND, "domain rule not found"),
        Err(e) => {
            tracing::error!("delete_domain_rule failed: {}", e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

// ── Overview ───────────────────────────────────────────────────────────────

/// `GET /api/federation/admin/overview`
pub async fn admin_overview(State(state): State<FederationState>, headers: HeaderMap) -> Response {
    require_admin!(headers);

    let remote_actors: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM federation_actors WHERE is_local = FALSE")
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let active_follows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM federation_follows WHERE status = 'accepted'")
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let remote_videos: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM remote_video_catalog WHERE is_deleted = FALSE")
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let pending_deliveries: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM federation_delivery_jobs WHERE status = 'queued'")
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let failed_deliveries: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM federation_delivery_jobs WHERE status = 'failed'")
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    let domain_rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM federation_domain_rules")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    Json(json!({
        "ok": true,
        "domain":             state.config.domain,
        "base_url":           state.config.base_url,
        "remote_actors":      remote_actors,
        "active_follows":     active_follows,
        "remote_videos":      remote_videos,
        "pending_deliveries": pending_deliveries,
        "failed_deliveries":  failed_deliveries,
        "domain_rules":       domain_rules,
    }))
    .into_response()
}

// ── Known instances ────────────────────────────────────────────────────────

/// `GET /api/federation/admin/instances`
///
/// Lists every remote domain that has at least one actor in this instance,
/// along with actor/video counts and any active domain rule.
pub async fn list_instances(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Query(q): Query<PaginationQuery>,
) -> Response {
    require_admin!(headers);

    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);

    let rows: Vec<(String, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT fa.domain,
                COUNT(DISTINCT fa.id)   AS actor_count,
                COUNT(DISTINCT rvc.id)  AS video_count,
                dr.action               AS domain_rule
         FROM federation_actors fa
         LEFT JOIN remote_video_catalog rvc
                ON rvc.origin_domain = fa.domain AND rvc.is_deleted = FALSE
         LEFT JOIN federation_domain_rules dr ON dr.domain = fa.domain
         WHERE fa.is_local = FALSE
         GROUP BY fa.domain, dr.action
         ORDER BY actor_count DESC, fa.domain
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let instances: Vec<Value> = rows
        .into_iter()
        .map(|(domain, actors, videos, rule)| {
            json!({
                "domain":       domain,
                "actor_count":  actors,
                "video_count":  videos,
                "domain_rule":  rule,
            })
        })
        .collect();

    Json(json!({ "ok": true, "instances": instances })).into_response()
}

// ── Activity log ───────────────────────────────────────────────────────────

/// `GET /api/federation/admin/activities`
pub async fn list_activities(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Query(q): Query<PaginationQuery>,
) -> Response {
    require_admin!(headers);

    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);

    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id::text, activity_type, actor_uri, direction, \
                    processing_status, error_message, created_at::text \
             FROM federation_activities \
             ORDER BY created_at DESC \
             LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let activities: Vec<Value> = rows
        .into_iter()
        .map(|(id, typ, actor, dir, status, error, created_at)| {
            json!({
                "id":           id,
                "type":         typ,
                "actor_uri":    actor,
                "direction":    dir,
                "status":       status,
                "error":        error,
                "created_at":   created_at,
            })
        })
        .collect();

    Json(json!({ "ok": true, "activities": activities })).into_response()
}

// ── Delivery queue ─────────────────────────────────────────────────────────

/// `GET /api/federation/admin/delivery`
pub async fn list_delivery_jobs(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Query(q): Query<PaginationQuery>,
) -> Response {
    require_admin!(headers);

    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);

    let rows: Vec<(
        String,
        String,
        i32,
        i32,
        String,
        Option<i32>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id::text, target_inbox_url, attempt_count, max_attempts, \
                status, last_http_status, last_error, next_attempt_at::text \
         FROM federation_delivery_jobs \
         ORDER BY next_attempt_at DESC \
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let jobs: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, inbox, attempts, max, status, http_status, error, next_at)| {
                json!({
                    "id":               id,
                    "target_inbox_url": inbox,
                    "attempt_count":    attempts,
                    "max_attempts":     max,
                    "status":           status,
                    "last_http_status": http_status,
                    "last_error":       error,
                    "next_attempt_at":  next_at,
                })
            },
        )
        .collect();

    Json(json!({ "ok": true, "jobs": jobs })).into_response()
}

// ── Failed delivery retry ──────────────────────────────────────────────────

/// `POST /api/federation/admin/delivery/:id/retry`
///
/// Reset a failed delivery job to `queued` so the background worker
/// retries it on the next cycle.  `attempt_count` is reset to zero so
/// the full retry budget is available again.
pub async fn retry_delivery_job(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> Response {
    require_admin!(headers);

    let id = match Uuid::parse_str(&job_id) {
        Ok(id) => id,
        Err(_) => return api_error(StatusCode::BAD_REQUEST, "invalid job id"),
    };

    let result = sqlx::query(
        "UPDATE federation_delivery_jobs \
         SET status = 'queued', attempt_count = 0, next_attempt_at = NOW(), \
             last_error = NULL, updated_at = NOW() \
         WHERE id = $1 AND status = 'failed'",
    )
    .bind(id)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => Json(json!({ "ok": true })).into_response(),
        Ok(_) => api_error(
            StatusCode::NOT_FOUND,
            "job not found or not in failed state",
        ),
        Err(e) => {
            tracing::error!("retry_delivery_job failed: {}", e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

// ── Cached remote content removal ─────────────────────────────────────────

/// `DELETE /api/federation/admin/remote-videos/:domain`
///
/// Soft-deletes all cached remote video index entries from `domain`.
/// The origin instance is not notified; if the domain rule is later
/// removed, re-published videos will be re-indexed through normal
/// Create/Update inbox activity processing.
pub async fn purge_remote_videos(
    State(state): State<FederationState>,
    headers: HeaderMap,
    Path(domain): Path<String>,
) -> Response {
    require_admin!(headers);

    let domain = domain.trim().to_lowercase();

    let result = sqlx::query(
        "UPDATE remote_video_catalog \
         SET is_deleted = TRUE, availability_status = 'deleted', updated_at = NOW() \
         WHERE origin_domain = $1 AND is_deleted = FALSE",
    )
    .bind(&domain)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) => {
            tracing::info!(%domain, removed = r.rows_affected(), "remote video cache purged");
            Json(json!({ "ok": true, "removed": r.rows_affected() })).into_response()
        }
        Err(e) => {
            tracing::error!("purge_remote_videos failed: {}", e);
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_must_be_bare_hostname() {
        // Simulate the validation logic from set_domain_rule
        let bad = ["example.com/path", "@user", ""];
        for d in bad {
            let domain = d.trim().to_lowercase();
            let invalid = domain.is_empty() || domain.contains('/') || domain.contains('@');
            assert!(invalid, "{d} should be invalid");
        }

        let good = "example.com";
        let domain = good.trim().to_lowercase();
        let invalid = domain.is_empty() || domain.contains('/') || domain.contains('@');
        assert!(!invalid, "{good} should be valid");
    }

    #[test]
    fn valid_actions_are_enumerated() {
        const VALID: &[&str] = &["allow", "silence", "reject_media", "suspend", "block"];
        for action in VALID {
            assert!(VALID.contains(action));
        }
        assert!(!VALID.contains(&"nuke"));
    }

    #[test]
    fn pagination_defaults() {
        let q: PaginationQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.limit, 50);
        assert_eq!(q.offset, 0);
    }
}
