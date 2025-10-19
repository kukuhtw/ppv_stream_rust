use axum::{http::StatusCode, response::IntoResponse};
pub async fn require_admin(is_admin: bool) -> impl IntoResponse {
    if !is_admin { return (StatusCode::FORBIDDEN, "admin only").into_response(); }
    (StatusCode::OK, "").into_response()
}
