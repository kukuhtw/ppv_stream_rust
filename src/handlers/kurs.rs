// src/handlers/kurs.rs
use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

use crate::config::Config;

#[derive(Clone)]
pub struct KursState {
    pub cfg: Config,
}

#[derive(Serialize)]
struct KursResp {
    ok: bool,
    usd_to_idr: f64,
    source: String,
    updated_at: String,
    stale: bool,
}

// GET /api/kurs -> { ok, usd_to_idr, source, updated_at, stale }
// The rate comes from crate::fx (live source or admin override), falling back
// to DOLLAR_USD_TO_RUPIAH when no rate is known.
async fn get_kurs(State(state): State<KursState>) -> Json<KursResp> {
    let r = crate::fx::usd_to_idr_info(state.cfg.dollar_usd_to_rupiah);
    Json(KursResp {
        ok: true,
        usd_to_idr: r.rate,
        source: r.source,
        updated_at: r.fetched_at.to_rfc3339(),
        stale: crate::fx::usd_to_idr_is_stale(),
    })
}

pub fn router(state: KursState) -> Router {
    Router::new()
        .route("/api/kurs", get(get_kurs))
        .with_state(state)
}
