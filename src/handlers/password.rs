use axum::{routing::post, Router};
use crate::handlers::auth_user;

pub fn routes() -> Router {
    Router::new()
        .route("/auth/forgot", post(auth_user::post_forgot))
        .route("/auth/reset", post(auth_user::post_reset))
}
