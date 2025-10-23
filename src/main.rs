// src/main.rs

use tracing_subscriber::fmt::init as tracing_init;

#[cfg(feature = "x402-watcher")]
mod services {
    pub mod x402_watcher;
}

// HANYA modul/mod yang dipakai langsung di file ini
mod config;
mod db;

mod email;
mod ffmpeg;
mod sessions;
mod validators;
mod worker;

mod handlers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_init();

    let cfg = config::Config::from_env();
    let pool = db::new_pool(&cfg.database_url).await?;

    // default: hanya HTTP server; watcher bisa diaktifkan via env WATCHER_ENABLE=1
    start_http_server(cfg, pool).await
}

async fn start_http_server(cfg: config::Config, pool: sqlx::PgPool) -> anyhow::Result<()> {
    // re-import lokal agar scope rapih
    use axum::{
        extract::DefaultBodyLimit,
        response::Redirect,
        routing::{get, post},
        Router,
    };
    use tokio::net::TcpListener;
    use tower_cookies::CookieManagerLayer;
    use tower_http::services::ServeDir;

    use crate::handlers::pay;
    use crate::handlers::me::{me, MeState};
    use crate::handlers::{
        admin::{admin_data, AdminState},
        auth_admin::{post_admin_login, post_admin_logout, AuthAdminState},
        auth_user::{post_login, post_logout, post_register, AuthUserState},
        kurs::{router as kurs_router, KursState},
        setup::{setup_admin, SetupState},
        stream::{request_play, serve_hls, StreamState},
        upload::{upload_video, UploadState},
        users::{get_my_profile, public_profile, update_my_profile, UsersState},
        video::{add_allow, list_videos, my_videos, update_video, user_lookup, VideoState},
    };
    use crate::worker;

    // ==== States ====
    let users_state = UsersState { pool: pool.clone() };
    let worker = worker::Worker::new(pool.clone(), cfg.clone(), 2);

    // ==== Static Files ====
    let static_service = ServeDir::new(&cfg.public_dir).append_index_html_on_directories(true);
    let hls_service = ServeDir::new(&cfg.media_dir);

    // ==== Static routes ====
    let static_router = Router::new()
        .route("/", get(|| async { Redirect::to("/public/") }))
        .route("/browse", get(|| async { Redirect::to("/public/") }))
        .route("/dashboard", get(|| async { Redirect::to("/public/dashboard.html") }))
        .route("/health", get(|| async { "ok" }))
        .nest_service("/public", static_service)
        .nest_service("/static_hls", hls_service);

    // ==== Admin pages ====
    let admin_pages_router = Router::new()
        .route("/admin/data", get(admin_data))
        .with_state(AdminState { pool: pool.clone() });

    // ==== User auth ====
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

    // ==== Admin auth ====
    let admin_auth_router = Router::new()
        .route(
            "/admin/login",
            get(|| async { Redirect::to("/public/admin/login.html") }).post(post_admin_login),
        )
        .route("/admin/logout", post(post_admin_logout))
        .with_state(AuthAdminState { pool: pool.clone() });

    // ==== Setup admin ====
    let setup_router = Router::new()
        .route("/setup_admin", get(setup_admin))
        .with_state(SetupState {
            pool: pool.clone(),
            token: std::env::var("ADMIN_BOOTSTRAP_TOKEN").ok(),
        });

    // ==== Upload (besar) ====
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

    // ==== Video & Payment routes ====
    let video_router = Router::new()
        .route("/api/videos", get(list_videos))
        .route("/api/my_videos", get(my_videos))
        .route("/api/user_lookup", get(user_lookup))
        .route("/api/allow", post(add_allow))
        .route("/api/video_update", post(update_video))
        .route("/api/pay/options", get(pay::pay_options))
        .route("/api/pay/x402/start", post(pay::x402_start))
        .route("/api/crypto_price", get(pay::crypto_price))
        .route("/api/pay/x402/confirm", post(pay::x402_confirm))
        .with_state(VideoState { pool: pool.clone() });

    // ==== Users ====
    let users_router = Router::new()
        .route("/api/profile", get(get_my_profile))
        .route("/api/profile_update", post(update_my_profile))
        .route("/api/user_profile", get(public_profile))
        .with_state(users_state);

    // ==== Streaming ====
    let streaming_router = Router::new()
        .route("/api/request_play", get(request_play))
        .route("/hls/:video/:file", get(serve_hls))
        .with_state(StreamState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ==== Me ====
    let me_router = Router::new()
        .route("/api/me", get(me))
        .with_state(MeState { pool: pool.clone() });

    // ==== Kurs ====
    let kurs_router = kurs_router(KursState { cfg: cfg.clone() });

    // ==== Merge all routers ====
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
        .merge(kurs_router)
        .layer(CookieManagerLayer::new());

    // ==== (opsional) Jalankan watcher di process yang sama bila di-enable ====
    #[cfg(feature = "x402-watcher")]
    if std::env::var("WATCHER_ENABLE").ok().as_deref() == Some("1") {
        use ethers::types::Address;
        use crate::services::x402_watcher::run_watcher;

        let pool_clone = pool.clone();
        if let (Ok(wss), Ok(addr_str)) = (
            std::env::var("X402_RPC_WSS"),
            std::env::var("X402_CONTRACT_ADDRESS"),
        ) {
            if !wss.is_empty() && !addr_str.is_empty() {
                if let Ok(addr) = addr_str.parse::<Address>() {
                    tokio::spawn(async move {
                        if let Err(e) = run_watcher(pool_clone, wss, addr).await {
                            tracing::error!("x402 watcher error: {}", e);
                        }
                    });
                }
            }
        }
    }

    // ==== Start server ====
    let addr = cfg.bind.clone();
    let listener = TcpListener::bind(&addr).await?;
    println!("listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
