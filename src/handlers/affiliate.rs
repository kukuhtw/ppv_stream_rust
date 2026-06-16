// src/handlers/affiliate.rs
//
// Affiliate system — user-facing and admin endpoints.
//
// Roles
//   Creator  (User A) — owns the video; configures commission % per video.
//   Affiliate (User B) — shares a referral link; earns commission when a buyer
//                        purchases through the link.
//   Buyer    (User C) — lands on watch.html?video_id=X&ref=USERNAME and buys.
//
// Referral link format: /public/watch.html?video_id=<id>&ref=<affiliate_username>
//
// Commission is taken from the creator's wallet balance and transferred to the
// affiliate after every successful purchase. See src/commission.rs.

use axum::{extract::{Query, State}, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use tower_cookies::Cookies;

use crate::config::Config;
use crate::sessions;

#[derive(Clone)]
pub struct AffiliateState {
    pub pool: PgPool,
    pub cfg:  Config,
}

// ─── GET /api/affiliate/settings?video_id= ───────────────────────────────────
// Returns current affiliate settings for a video. Creator-only.

pub async fn affiliate_settings_get(
    State(st): State<AffiliateState>,
    cookies:   Cookies,
    Query(q):  Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None    => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let video_id = q.get("video_id").cloned().unwrap_or_default();
    if video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    let owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM videos WHERE id = $1 AND owner_id = $2"
    )
    .bind(&video_id).bind(&uid)
    .fetch_one(&st.pool).await.unwrap_or(0);

    if owned == 0 {
        return Json(json!({"ok": false, "error": "not your video"}));
    }

    let row = sqlx::query(
        "SELECT commission_pct, is_enabled \
         FROM affiliate_settings WHERE video_id = $1 LIMIT 1"
    )
    .bind(&video_id)
    .fetch_optional(&st.pool)
    .await;

    match row {
        Ok(Some(r)) => Json(json!({
            "ok":           true,
            "video_id":     video_id,
            "commission_pct": r.try_get::<i32, _>("commission_pct").unwrap_or(0),
            "is_enabled":   r.try_get::<bool, _>("is_enabled").unwrap_or(false),
        })),
        Ok(None) => Json(json!({
            "ok":             true,
            "video_id":       video_id,
            "commission_pct": 0,
            "is_enabled":     false,
        })),
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

// ─── POST /api/affiliate/settings ────────────────────────────────────────────
// Upsert affiliate settings. Creator-only.

#[derive(Deserialize)]
pub struct AffiliateSettingsPayload {
    pub video_id:       String,
    pub commission_pct: i32,
    pub is_enabled:     bool,
}

pub async fn affiliate_settings_save(
    State(st): State<AffiliateState>,
    cookies:   Cookies,
    Json(p):   Json<AffiliateSettingsPayload>,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None    => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    if p.video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }
    if !(0..=90).contains(&p.commission_pct) {
        return Json(json!({"ok": false, "error": "commission_pct must be 0–90"}));
    }

    let owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM videos WHERE id = $1 AND owner_id = $2"
    )
    .bind(&p.video_id).bind(&uid)
    .fetch_one(&st.pool).await.unwrap_or(0);

    if owned == 0 {
        return Json(json!({"ok": false, "error": "not your video"}));
    }

    let res = sqlx::query(
        r#"INSERT INTO affiliate_settings (video_id, owner_id, commission_pct, is_enabled)
           VALUES ($1, $2, $3, $4)
           ON CONFLICT (video_id) DO UPDATE
             SET commission_pct = EXCLUDED.commission_pct,
                 is_enabled     = EXCLUDED.is_enabled,
                 updated_at     = NOW()"#
    )
    .bind(&p.video_id).bind(&uid).bind(p.commission_pct).bind(p.is_enabled)
    .execute(&st.pool)
    .await;

    match res {
        Ok(_)  => Json(json!({"ok": true, "message": "Affiliate settings saved."})),
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

// ─── GET /api/affiliate/link?video_id= ───────────────────────────────────────
// Returns the referral URL for the current user for a specific video.

pub async fn affiliate_link(
    State(st): State<AffiliateState>,
    cookies:   Cookies,
    Query(q):  Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let (_, username) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None    => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let video_id = q.get("video_id").cloned().unwrap_or_default();
    if video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    let enabled: bool = sqlx::query_scalar(
        "SELECT is_enabled FROM affiliate_settings WHERE video_id = $1"
    )
    .bind(&video_id)
    .fetch_optional(&st.pool)
    .await
    .ok().flatten().unwrap_or(false);

    let link = format!("/public/watch.html?video_id={video_id}&ref={username}");

    Json(json!({
        "ok":         true,
        "link":       link,
        "username":   username,
        "video_id":   video_id,
        "is_enabled": enabled,
    }))
}

// ─── GET /api/affiliate/earnings ─────────────────────────────────────────────
// Lists commissions earned by the current user as an affiliate.

pub async fn affiliate_earnings(
    State(st): State<AffiliateState>,
    cookies:   Cookies,
    Query(q):  Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let (uid, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None    => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    let limit: i64 = q.get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50)
        .min(200);

    let rows = sqlx::query(
        r#"SELECT ac.id,
                  ac.video_id,
                  ac.commission_cents,
                  ac.purchase_price_cents,
                  ac.payment_method,
                  ac.created_at::TEXT AS created_at,
                  v.title             AS video_title,
                  buyer.username      AS buyer_username
           FROM affiliate_commissions ac
           JOIN videos v      ON v.id      = ac.video_id
           JOIN users  buyer  ON buyer.id  = ac.buyer_id
           WHERE ac.affiliate_id = $1
           ORDER BY ac.created_at DESC
           LIMIT $2"#
    )
    .bind(&uid).bind(limit)
    .fetch_all(&st.pool)
    .await;

    match rows {
        Ok(r) => {
            let total_cents: i64 = r.iter()
                .map(|row| row.try_get::<i64, _>("commission_cents").unwrap_or(0))
                .sum();

            let fmt = |c: i64| format!("${}.{:02}", c / 100, (c % 100).unsigned_abs());

            let items: Vec<serde_json::Value> = r.iter().map(|row| json!({
                "id":                   row.try_get::<i64, _>("id").unwrap_or(0),
                "video_id":             row.try_get::<String, _>("video_id").unwrap_or_default(),
                "video_title":          row.try_get::<String, _>("video_title").unwrap_or_default(),
                "buyer_username":       row.try_get::<String, _>("buyer_username").unwrap_or_default(),
                "commission_cents":     row.try_get::<i64, _>("commission_cents").unwrap_or(0),
                "commission_display":   fmt(row.try_get::<i64, _>("commission_cents").unwrap_or(0)),
                "purchase_price_cents": row.try_get::<i64, _>("purchase_price_cents").unwrap_or(0),
                "payment_method":       row.try_get::<String, _>("payment_method").unwrap_or_default(),
                "created_at":           row.try_get::<Option<String>, _>("created_at")
                                            .unwrap_or(None).unwrap_or_default(),
            })).collect();

            Json(json!({
                "ok":                    true,
                "total_commission_cents": total_cents,
                "total_display":         fmt(total_cents),
                "items":                 items,
            }))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

// ─── GET /api/affiliate/program?video_id= ────────────────────────────────────
// Public: returns whether a video has an active affiliate program and its
// commission %. Used by watch.html to show "Earn commission" badge.

pub async fn affiliate_program_info(
    State(st): State<AffiliateState>,
    Query(q):  Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let video_id = q.get("video_id").cloned().unwrap_or_default();
    if video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    let row = sqlx::query(
        "SELECT commission_pct, is_enabled \
         FROM affiliate_settings WHERE video_id = $1 LIMIT 1"
    )
    .bind(&video_id)
    .fetch_optional(&st.pool)
    .await;

    match row {
        Ok(Some(r)) => {
            let enabled: bool = r.try_get("is_enabled").unwrap_or(false);
            let pct:     i32  = r.try_get("commission_pct").unwrap_or(0);
            Json(json!({
                "ok":             true,
                "has_affiliate":  enabled && pct > 0,
                "commission_pct": pct,
            }))
        }
        Ok(None) => Json(json!({"ok": true, "has_affiliate": false, "commission_pct": 0})),
        Err(e)   => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}

// ─── GET /admin/affiliate/commissions ────────────────────────────────────────
// Admin view: all commissions across the platform.

pub async fn admin_affiliate_commissions(
    State(st): State<AffiliateState>,
    Query(q):  Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let limit: i64 = q.get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
        .min(500);

    let rows = sqlx::query(
        r#"SELECT ac.id,
                  ac.video_id,
                  ac.commission_cents,
                  ac.purchase_price_cents,
                  ac.payment_method,
                  ac.created_at::TEXT  AS created_at,
                  v.title              AS video_title,
                  aff.username         AS affiliate_username,
                  buyer.username       AS buyer_username,
                  owner.username       AS owner_username
           FROM affiliate_commissions ac
           JOIN videos v      ON v.id     = ac.video_id
           JOIN users  aff    ON aff.id   = ac.affiliate_id
           JOIN users  buyer  ON buyer.id = ac.buyer_id
           JOIN users  owner  ON owner.id = ac.owner_id
           ORDER BY ac.created_at DESC
           LIMIT $1"#
    )
    .bind(limit)
    .fetch_all(&st.pool)
    .await;

    match rows {
        Ok(r) => {
            let total_cents: i64 = r.iter()
                .map(|row| row.try_get::<i64, _>("commission_cents").unwrap_or(0))
                .sum();

            let fmt = |c: i64| format!("${}.{:02}", c / 100, (c % 100).unsigned_abs());

            let items: Vec<serde_json::Value> = r.iter().map(|row| json!({
                "id":                   row.try_get::<i64, _>("id").unwrap_or(0),
                "video_id":             row.try_get::<String, _>("video_id").unwrap_or_default(),
                "video_title":          row.try_get::<String, _>("video_title").unwrap_or_default(),
                "affiliate_username":   row.try_get::<String, _>("affiliate_username").unwrap_or_default(),
                "buyer_username":       row.try_get::<String, _>("buyer_username").unwrap_or_default(),
                "owner_username":       row.try_get::<String, _>("owner_username").unwrap_or_default(),
                "commission_cents":     row.try_get::<i64, _>("commission_cents").unwrap_or(0),
                "commission_display":   fmt(row.try_get::<i64, _>("commission_cents").unwrap_or(0)),
                "purchase_price_cents": row.try_get::<i64, _>("purchase_price_cents").unwrap_or(0),
                "payment_method":       row.try_get::<String, _>("payment_method").unwrap_or_default(),
                "created_at":           row.try_get::<Option<String>, _>("created_at")
                                            .unwrap_or(None).unwrap_or_default(),
            })).collect();

            Json(json!({
                "ok":                    true,
                "total_commission_cents": total_cents,
                "total_display":         fmt(total_cents),
                "count":                 items.len(),
                "items":                 items,
            }))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("db: {e}")})),
    }
}
