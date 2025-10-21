// src/main.rs

use axum::{
    extract::DefaultBodyLimit,
    response::Redirect,
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::fmt::init as tracing_init;

mod config;
mod db;
mod email;
mod ffmpeg; // masih dipakai oleh worker/utility
mod sessions;
mod validators;
mod worker;

mod handlers;
use handlers::me::{me, MeState};
use handlers::{
    admin::{admin_data, AdminState},
    auth_admin::{post_admin_login, post_admin_logout, AuthAdminState},
    auth_user::{post_login, post_logout, post_register, AuthUserState},
    setup::{setup_admin, SetupState},
    stream::{request_play, serve_hls, StreamState},
    upload::{upload_video, UploadState},
    video::{add_allow, list_videos, my_videos, update_video, user_lookup, VideoState},
    users::{get_my_profile, public_profile, update_my_profile, UsersState},
};

// 👉 pastikan modul handlers::kurs sudah dibuat dan diexport (pub mod kurs; di handlers/mod.rs)
use handlers::kurs::{router as kurs_router, KursState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_init();

    // ==== Config & DB ====
    let cfg = config::Config::from_env();
    // Catatan: sesuaikan signature new_pool dengan implementasi kamu.
    // Di sini diasumsikan new_pool(&str) -> PgPool
    let pool = db::new_pool(&cfg.database_url).await?;

    // ==== States ====
    let users_state = UsersState { pool: pool.clone() };

    // ==== Worker (transcode HLS pasca-upload) ====
    // Argumen terakhir = jumlah worker paralel (sesuaikan kebutuhan)
    let worker = worker::Worker::new(pool.clone(), cfg.clone(), 2);

    // ==== Static files (/public) ====
    let public_root = cfg.public_dir.clone();
    let static_service = ServeDir::new(public_root).append_index_html_on_directories(true);

    // ==== Static HLS (/static_hls) dari MEDIA_DIR ====
    // File hasil transcode HLS disajikan langsung (cepat & non-blok).
    // Gunakan prefix berbeda agar tidak konflik dengan /hls/:video/:file
    let hls_service = ServeDir::new(&cfg.media_dir);

    // ===== Static/router dasar =====
    let static_router = Router::new()
        .route("/", get(|| async { Redirect::to("/public/") }))
        .route("/browse", get(|| async { Redirect::to("/public/") }))
        .route("/dashboard", get(|| async { Redirect::to("/public/dashboard.html") }))
        .route("/health", get(|| async { "ok" }))
        .nest_service("/public", static_service)
        .nest_service("/static_hls", hls_service); // aman: tidak bentrok dengan /hls/:video/:file

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
        .with_state(UploadState {
            cfg: cfg.clone(),
            pool: pool.clone(),
            worker: worker.clone(),
        })
        .layer(DefaultBodyLimit::max(
            cfg.max_upload_bytes.try_into().unwrap_or(usize::MAX),
        ));

    // ===== Video (listing, my_videos, allowlist, update) =====
    let video_router = Router::new()
        .route("/api/videos", get(list_videos))
        .route("/api/my_videos", get(my_videos))
        .route("/api/user_lookup", get(user_lookup))
        .route("/api/allow", post(add_allow))
        .route("/api/video_update", post(update_video))
        .with_state(VideoState { pool: pool.clone() });

    // ===== Users (profile) =====
    let users_router = Router::new()
        .route("/api/profile", get(get_my_profile))
        .route("/api/profile_update", post(update_my_profile))
        .route("/api/user_profile", get(public_profile))
        .with_state(users_state);

    // ===== Streaming (request_play + serve_hls manual) =====
    // Tetap pakai handler serve_hls agar bisa kontrol akses/logging.
    let streaming_router = Router::new()
        .route("/api/request_play", get(request_play))
        .route("/hls/:video/:file", get(serve_hls))
        .with_state(StreamState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ===== Me (info user login: id, username, email) =====
    let me_router = Router::new()
        .route("/api/me", get(me))
        .with_state(MeState { pool: pool.clone() });

    // ===== Kurs (expose kurs dari Config ke frontend) =====
    // pastikan handlers::kurs::router tersedia
    let kurs_router = kurs_router(KursState { cfg: cfg.clone() });

    // ===== Merge + cookies =====
    let app = static_router
        .merge(admin_pages_router)
        .merge(user_auth_router)
        .merge(admin_auth_router)
        .merge(setup_router)
        .merge(upload_router)
        .merge(video_router)
        .merge(users_router)
        .merge(streaming_router)
        .merge(me_router)
        // 👉 mount kurs agar /api/kurs tersedia untuk watch/browse (harga Rupiah sinkron .env)
        .merge(kurs_router)
        .layer(CookieManagerLayer::new());

    // ==== Start server ====
    let addr = cfg.bind.clone(); // contoh: "0.0.0.0:8080"
    let listener = TcpListener::bind(&addr).await?;
    println!("listening on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
