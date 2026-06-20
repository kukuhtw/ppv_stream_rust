use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::FederationState;

/// Combined local + remote video catalog entry.
#[derive(Debug, Serialize)]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub price_cents: Option<i64>,
    pub hosting_type: &'static str,
    pub origin_domain: String,
    pub watch_url: String,
    pub checkout_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub content_rating: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    /// Filter: "local" | "remote" | "" (all)
    #[serde(default)]
    hosting_type: String,
}

fn default_limit() -> i64 {
    20
}

/// `GET /api/federation/catalog`
///
/// Returns a unified list of public local videos and remote video index entries.
/// Remote video entries include canonical origin URLs; clients must redirect to
/// the origin for playback and payment — local media serving is rejected.
pub async fn catalog(
    State(state): State<FederationState>,
    Query(q): Query<CatalogQuery>,
) -> Response {
    let limit = q.limit.clamp(1, 100);
    let offset = q.offset.max(0);

    let mut entries: Vec<CatalogEntry> = Vec::new();

    // ── Local public videos ────────────────────────────────────────────────
    if q.hosting_type.is_empty() || q.hosting_type == "local" {
        let local_rows: Vec<(
            String,        // id
            String,        // title
            String,        // description
            i64,           // price_cents
            Option<String>,// object_uri
        )> = sqlx::query_as(
            "SELECT v.id, v.title, v.description, v.price_cents, v.object_uri \
             FROM videos v \
             WHERE v.federation_visibility = 'public' \
             ORDER BY v.created_at DESC \
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let base = &state.config.base_url;
        let domain = &state.config.domain;

        for (id, title, description, price_cents, _object_uri) in local_rows {
            entries.push(CatalogEntry {
                id: id.clone(),
                title,
                description: if description.is_empty() {
                    None
                } else {
                    Some(description)
                },
                price_cents: Some(price_cents),
                hosting_type: "local",
                origin_domain: domain.clone(),
                watch_url: format!("{}/watch/{}", base, id),
                checkout_url: Some(format!("{}/checkout/{}", base, id)),
                thumbnail_url: None,
                content_rating: None,
                published_at: None,
            });
        }
    }

    // ── Remote video catalog ───────────────────────────────────────────────
    if q.hosting_type.is_empty() || q.hosting_type == "remote" {
        let remote_rows: Vec<(
            String,         // object_uri
            String,         // title
            Option<String>, // description
            String,         // canonical_url
            Option<String>, // checkout_url
            Option<String>, // thumbnail_url
            String,         // origin_domain
            Option<String>, // content_rating
            Option<String>, // published_at (as text from TIMESTAMPTZ)
        )> = sqlx::query_as(
            "SELECT object_uri, title, description, canonical_url, checkout_url, \
                    thumbnail_url, origin_domain, content_rating, \
                    TO_CHAR(published_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') \
             FROM remote_video_catalog \
             WHERE is_deleted = FALSE AND availability_status = 'available' \
             ORDER BY published_at DESC NULLS LAST, updated_at DESC \
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        for (
            object_uri, title, description, canonical_url, checkout_url,
            thumbnail_url, origin_domain, content_rating, published_at,
        ) in remote_rows
        {
            entries.push(CatalogEntry {
                id: object_uri,
                title,
                description,
                price_cents: None, // remote: price shown via checkout_url
                hosting_type: "remote",
                origin_domain,
                watch_url: canonical_url,
                checkout_url,
                thumbnail_url,
                content_rating,
                published_at,
            });
        }
    }

    if entries.is_empty() && !q.hosting_type.is_empty()
        && q.hosting_type != "local"
        && q.hosting_type != "remote"
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "hosting_type must be 'local', 'remote', or omitted" })),
        )
            .into_response();
    }

    let count = entries.len();
    Json(json!({
        "items": entries,
        "count": count
    }))
    .into_response()
}

/// `GET /videos/:id` with ActivityPub content negotiation.
///
/// When the client sends `Accept: application/activity+json`, returns the
/// ActivityPub Video object for a local public video.
pub async fn video_ap_object(
    State(state): State<FederationState>,
    axum::extract::Path(video_id): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> Response {
    let accept = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if !accept.contains("application/activity+json")
        && !accept.contains("application/ld+json")
    {
        // Not an ActivityPub request — let the HTML handler take it
        return (StatusCode::NOT_ACCEPTABLE, "use Accept: application/activity+json").into_response();
    }

    match crate::federation::video_index::build_video_object(
        &state.pool,
        &video_id,
        &state.config.base_url,
    )
    .await
    {
        Ok(Some(obj)) => {
            let mut resp_headers = axum::http::HeaderMap::new();
            resp_headers.insert(
                axum::http::header::CONTENT_TYPE,
                "application/activity+json; charset=utf-8".parse().unwrap(),
            );
            (resp_headers, Json(obj)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "video not found or not federated" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("video AP object error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to build video object" })),
            )
                .into_response()
        }
    }
}
