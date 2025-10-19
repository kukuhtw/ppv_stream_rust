use axum::response::{Html, IntoResponse};

pub async fn dashboard() -> impl IntoResponse { Html(include_str!("../../public/dashboard.html")) }
pub async fn browse() -> impl IntoResponse { Html(include_str!("../../public/browse.html")) }
pub async fn watch() -> impl IntoResponse { Html(include_str!("../../public/watch.html")) }
