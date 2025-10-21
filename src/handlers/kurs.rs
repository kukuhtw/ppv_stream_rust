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
}

// GET /api/kurs -> { ok: true, usd_to_idr: <f64> }
async fn get_kurs(State(state): State<KursState>) -> Json<KursResp> {
    Json(KursResp {
        ok: true,
        usd_to_idr: state.cfg.dollar_usd_to_rupiah,
    })
}

pub fn router(state: KursState) -> Router {
    Router::new().route("/api/kurs", get(get_kurs)).with_state(state)
}
