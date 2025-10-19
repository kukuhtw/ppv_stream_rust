// src/main.rs
use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    response::Redirect,
    Router,
};
use tokio::net::TcpListener;
use tracing_subscriber::fmt::init as tracing_init;
use tower_http::services::ServeDir;
use tower_cookies::CookieManagerLayer;

mod handlers;
use handlers::me::{me, MeState};

mod config;
mod db;
mod sessions;
mod validators;
mod email;
mod ffmpeg;            // ✅ diperlukan oleh stream

use handlers::{
    admin::{admin_data, AdminState},
    auth_admin::{post_admin_login, post_admin_logout, AuthAdminState},
    auth_user::{post_login, post_logout, post_register, AuthUserState},
    setup::{setup_admin, SetupState},
    stream::{request_play, serve_hls, StreamState},
    upload::{upload_video, UploadState},
    video::{add_allow, list_videos, my_videos, user_lookup, VideoState},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_init();

    let cfg = config::Config::from_env();
    let pool = db::new_pool(&cfg.database_url).await?;

    // ===== Static files (/public) + root redirect + health =====
    let public_root = std::env::var("PUBLIC_DIR")
        .unwrap_or_else(|_| format!("{}/public", env!("CARGO_MANIFEST_DIR")));
    let static_service = ServeDir::new(public_root).append_index_html_on_directories(true);

    let static_router = Router::new()
        .route("/", get(|| async { Redirect::to("/public/") }))
        .route("/browse", get(|| async { Redirect::to("/public/") }))
        .route("/dashboard", get(|| async { Redirect::to("/public/dashboard.html") }))
        .route("/health", get(|| async { "ok" }))
        .nest_service("/public", static_service);

    // ===== Admin pages =====
    let admin_pages_router = Router::new()
        .route("/admin/data", get(admin_data))
        .with_state(AdminState { pool: pool.clone() });

    // ===== User auth =====
    let user_auth_router = Router::new()
        .route(
            "/auth/register",
            get(|| async { Redirect::to("/public/auth/register.html") }).post(post_register),
        )
        .route(
            "/auth/login",
            get(|| async { Redirect::to("/public/auth/login.html") }).post(post_login),
        )
        .route("/auth/logout", post(post_logout))
        .route(
            "/auth/forgot",
            get(|| async { Redirect::to("/public/auth/forgot_password.html") }),
        )
        .with_state(AuthUserState { pool: pool.clone() });

    // ===== Admin auth =====
    let admin_auth_router = Router::new()
        .route(
            "/admin/login",
            get(|| async { Redirect::to("/public/admin/login.html") }).post(post_admin_login),
        )
        .route("/admin/logout", post(post_admin_logout))
        .with_state(AuthAdminState { pool: pool.clone() });

    // ===== Setup (bootstrap admin) =====
    let setup_router = Router::new()
        .route("/setup_admin", get(setup_admin))
        .with_state(SetupState {
            pool: pool.clone(),
            token: std::env::var("ADMIN_BOOTSTRAP_TOKEN").ok(),
        });

    // ===== Upload (besar) =====
    let upload_router = Router::new()
        .route("/api/upload", post(upload_video))
        .with_state(UploadState { cfg: cfg.clone(), pool: pool.clone() })
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024)); // 1 GB

    // ===== Video (listing, my_videos, allowlist) =====
    let video_router = Router::new()
        .route("/api/videos", get(list_videos))
        .route("/api/my_videos", get(my_videos))
        .route("/api/user_lookup", get(user_lookup))
        .route("/api/allow", post(add_allow))
        .with_state(VideoState { pool: pool.clone() });

    // ===== Streaming (request_play + serve_hls) =====
    let streaming_router = Router::new()
        .route("/api/request_play", get(request_play))
        .route("/hls/:session/:file", get(serve_hls))
        .with_state(StreamState { pool: pool.clone(), cfg: cfg.clone() });

    // ===== Me (info user login: id, username, email) =====
    let me_router = Router::new()
        .route("/api/me", get(me))
        .with_state(MeState { pool: pool.clone() }); // ✅ pakai `pool`, bukan `pg_pool`

    // ===== Merge + cookies =====
    let app = static_router
        .merge(admin_pages_router)
        .merge(user_auth_router)
        .merge(admin_auth_router)
        .merge(setup_router)
        .merge(upload_router)
        .merge(video_router)
        .merge(streaming_router)
        .merge(me_router)
        .layer(CookieManagerLayer::new());

    let addr = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = TcpListener::bind(&addr).await?;
    println!("listening on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
