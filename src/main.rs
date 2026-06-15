// src/main.rs
// Application entry point and HTTP server composition root.
//
// This file is responsible for:
// 1. Initializing application logging.
// 2. Loading runtime configuration from environment variables.
// 3. Creating the PostgreSQL connection pool.
// 4. Building and combining all HTTP route groups.
// 5. Starting the Axum web server.
// 6. Optionally starting the x402 blockchain payment watcher.

use tracing_subscriber::fmt::init as tracing_init;

// The x402 watcher module is compiled only when the `x402-watcher`
// Cargo feature is enabled. This keeps blockchain-specific dependencies
// optional for deployments that only need the HTTP server.
#[cfg(feature = "x402-watcher")]
mod services {
    pub mod x402_watcher;
}

// Core application modules used by the server bootstrap process.
mod config;
mod db;

// Supporting business and infrastructure modules.
mod email;
mod ffmpeg;
mod sessions;
mod validators;
mod worker;

// HTTP request handlers grouped by business capability.
mod handlers;

/// Starts the application runtime.
///
/// The startup sequence is intentionally kept small:
/// initialize logging, load configuration, connect to PostgreSQL,
/// and delegate HTTP server construction to `start_http_server`.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Enable structured application logging through the `tracing` ecosystem.
    tracing_init();

    // Load application settings such as database URL, bind address,
    // public directories, media directories, and upload limits.
    let cfg = config::Config::from_env();

    // Create a reusable asynchronous PostgreSQL connection pool.
    // Cloned pool handles share the same underlying connection pool.
    let pool = db::new_pool(&cfg.database_url).await?;

    // Run the HTTP server. By default, only the web server is started.
    // The optional x402 watcher is started later when explicitly enabled.
    start_http_server(cfg, pool).await
}

/// Builds all application routers, attaches shared state and middleware,
/// optionally starts background services, and serves HTTP requests.
async fn start_http_server(cfg: config::Config, pool: sqlx::PgPool) -> anyhow::Result<()> {
    use axum::{
        extract::DefaultBodyLimit,
        response::Redirect,
        routing::{get, post},
        Router,
    };
    use tokio::net::TcpListener;
    use tower_cookies::CookieManagerLayer;
    use tower_http::services::ServeDir;

    // Import payment handlers separately because the payment module exposes
    // several related endpoints under the same route group.
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

    // ---------------------------------------------------------------------
    // Shared application state
    // ---------------------------------------------------------------------

    // State used by profile-related handlers. It contains both the database
    // pool and application configuration needed by user operations.
    let users_state = UsersState {
        pool: pool.clone(),
        cfg: cfg.clone(),
    };

    // Create the background video processing worker.
    // The final argument controls the number of worker tasks or processing slots.
    let worker = worker::Worker::new(pool.clone(), cfg.clone(), 2);

    // ---------------------------------------------------------------------
    // Static file services
    // ---------------------------------------------------------------------

    // Serve frontend files such as HTML, CSS, JavaScript, and images.
    // Directory requests automatically resolve to an index.html file.
    let static_service = ServeDir::new(&cfg.public_dir).append_index_html_on_directories(true);

    // Serve generated HLS playlists and media segments from the media directory.
    let hls_service = ServeDir::new(&cfg.media_dir);

    // Public page routes and operational health endpoint.
    let static_router = Router::new()
        // Redirect the root URL to the static frontend entry point.
        .route("/", get(|| async { Redirect::to("/public/") }))
        // `/browse` currently uses the same static frontend landing page.
        .route("/browse", get(|| async { Redirect::to("/public/") }))
        // Open the user dashboard page.
        .route("/dashboard", get(|| async { Redirect::to("/public/dashboard.html") }))
        // Lightweight endpoint for load balancers and uptime monitoring.
        .route("/health", get(|| async { "ok" }))
        // Mount public frontend assets under `/public`.
        .nest_service("/public", static_service)
        // Mount raw static HLS files under `/static_hls` when direct access is needed.
        .nest_service("/static_hls", hls_service);

    // ---------------------------------------------------------------------
    // Administration routes
    // ---------------------------------------------------------------------

    // Returns administration data to authorized administrator clients.
    let admin_pages_router = Router::new()
        .route("/admin/data", get(admin_data))
        .with_state(AdminState { pool: pool.clone() });

    // ---------------------------------------------------------------------
    // End-user authentication routes
    // ---------------------------------------------------------------------

    // GET requests open the corresponding static form pages.
    // POST requests execute registration, login, or logout logic.
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
        // Authentication handlers need database access and security configuration.
        .with_state(AuthUserState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ---------------------------------------------------------------------
    // Administrator authentication routes
    // ---------------------------------------------------------------------

    // Provides a dedicated login and logout flow for administrators.
    let admin_auth_router = Router::new()
        .route(
            "/admin/login",
            get(|| async { Redirect::to("/public/admin/login.html") }).post(post_admin_login),
        )
        .route("/admin/logout", post(post_admin_logout))
        .with_state(AuthAdminState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ---------------------------------------------------------------------
    // Initial administrator bootstrap
    // ---------------------------------------------------------------------

    // Creates or initializes the first administrator account.
    // Access can be protected through the `ADMIN_BOOTSTRAP_TOKEN`
    // environment variable.
    let setup_router = Router::new()
        .route("/setup_admin", get(setup_admin))
        .with_state(SetupState {
            pool: pool.clone(),
            token: std::env::var("ADMIN_BOOTSTRAP_TOKEN").ok(),
        });

    // ---------------------------------------------------------------------
    // Video upload route
    // ---------------------------------------------------------------------

    // Accepts large video uploads and submits processing work to the worker.
    // A route-specific body limit is used because video files can be much larger
    // than ordinary API request payloads.
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

    // ---------------------------------------------------------------------
    // Video catalog, authorization, and payment routes
    // ---------------------------------------------------------------------

    // These endpoints manage video listings, owner-specific videos,
    // playback authorization, metadata updates, and x402 payment operations.
    let video_router = Router::new()
        // List videos available to the current caller.
        .route("/api/videos", get(list_videos))
        // List videos owned by the authenticated user.
        .route("/api/my_videos", get(my_videos))
        // Search for a user when configuring access permissions.
        .route("/api/user_lookup", get(user_lookup))
        // Grant a user access to a protected video.
        .route("/api/allow", post(add_allow))
        // Update video metadata or settings.
        .route("/api/video_update", post(update_video))
        // Return supported payment methods and payment configuration.
        .route("/api/pay/options", get(pay::pay_options))
        // Start an x402 cryptocurrency payment request.
        .route("/api/pay/x402/start", post(pay::x402_start))
        // Return the current cryptocurrency conversion price used by checkout.
        .route("/api/crypto_price", get(pay::crypto_price))
        // Confirm or verify the completed x402 payment.
        .route("/api/pay/x402/confirm", post(pay::x402_confirm))
        .with_state(VideoState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ---------------------------------------------------------------------
    // User profile routes
    // ---------------------------------------------------------------------

    // Manage the authenticated user's profile and expose selected public
    // profile information for other users.
    let users_router = Router::new()
        .route("/api/profile", get(get_my_profile))
        .route("/api/profile_update", post(update_my_profile))
        .route("/api/user_profile", get(public_profile))
        .with_state(users_state);

    // ---------------------------------------------------------------------
    // Protected streaming routes
    // ---------------------------------------------------------------------

    // `request_play` validates access and prepares a playback session.
    // `serve_hls` serves HLS playlists or media segments using both the
    // playback session identifier and requested file name.
    let streaming_router = Router::new()
        .route("/api/request_play", get(request_play))
        .route("/hls/:session/:file", get(serve_hls))
        .with_state(StreamState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ---------------------------------------------------------------------
    // Current authenticated user route
    // ---------------------------------------------------------------------

    // Returns a compact representation of the active login session.
    let me_router = Router::new()
        .route("/api/me", get(me))
        .with_state(MeState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    // ---------------------------------------------------------------------
    // Currency exchange routes
    // ---------------------------------------------------------------------

    // Builds endpoints used to obtain exchange-rate data required by payment flows.
    let kurs_router = kurs_router(KursState { cfg: cfg.clone() });

    // ---------------------------------------------------------------------
    // Final application router
    // ---------------------------------------------------------------------

    // Merge all independent route groups into one Axum application.
    // Cookie middleware is added globally so authentication handlers can
    // read, create, and remove session cookies.
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

    // ---------------------------------------------------------------------
    // Optional x402 blockchain watcher
    // ---------------------------------------------------------------------

    // This block exists only in builds compiled with the `x402-watcher` feature.
    // When `WATCHER_ENABLE=1`, it opens a WebSocket connection to the blockchain
    // RPC endpoint and watches the configured smart contract for payment events.
    #[cfg(feature = "x402-watcher")]
    if std::env::var("WATCHER_ENABLE").ok().as_deref() == Some("1") {
        use crate::services::x402_watcher::run_watcher;
        use ethers::types::Address;

        let pool_clone = pool.clone();

        // Both values are required before the watcher can start:
        // `X402_RPC_WSS` for the blockchain WebSocket endpoint and
        // `X402_CONTRACT_ADDRESS` for the payment contract address.
        if let (Ok(wss), Ok(addr_str)) = (
            std::env::var("X402_RPC_WSS"),
            std::env::var("X402_CONTRACT_ADDRESS"),
        ) {
            if !wss.is_empty() && !addr_str.is_empty() {
                if let Ok(addr) = addr_str.parse::<Address>() {
                    // Run the watcher concurrently so it does not block
                    // startup or request handling by the HTTP server.
                    tokio::spawn(async move {
                        if let Err(e) = run_watcher(pool_clone, wss, addr).await {
                            tracing::error!("x402 watcher error: {}", e);
                        }
                    });
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // Start listening for HTTP connections
    // ---------------------------------------------------------------------

    // Bind to the configured host and port, then serve the composed Axum router.
    let addr = cfg.bind.clone();
    let listener = TcpListener::bind(&addr).await?;
    println!("listening on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
