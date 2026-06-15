// src/main.rs
use tracing_subscriber::fmt::init as tracing_init;

#[cfg(feature = "x402-watcher")]
mod services {
    pub mod x402_watcher;
}

mod config;
mod db;
mod email;
mod ffmpeg;
mod sessions;
mod validators;
mod worker;
mod handlers;
mod plugins;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_init();
    let cfg = config::Config::from_env();
    let pool = db::new_pool(&cfg.database_url).await?;
    start_http_server(cfg, pool).await
}

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

    use crate::handlers::pay;
    use crate::handlers::me::{me, MeState};
    use crate::handlers::{
        admin::{admin_data, admin_disburse, admin_payments, admin_smtp_get, admin_smtp_save, AdminState},
        auth_admin::{admin_change_password, post_admin_login, post_admin_logout, AuthAdminState},
        auth_user::{change_password, post_login, post_logout, post_register, AuthUserState},
        kurs::{router as kurs_router, KursState},
        payment_plugins::{
            confirm_default_payment, confirm_payment, create_default_payment_invoice,
            create_payment_invoice, handle_webhook, list_payment_plugins, PaymentPluginState,
        },
        setup::{setup_admin, SetupState},
        stream::{request_play, serve_hls, start_cleanup_task, StreamState},
        upload::{upload_video, UploadState},
        users::{get_my_profile, public_profile, update_my_profile, UsersState},
        video::{add_allow, list_videos, my_videos, update_video, user_lookup, VideoState},
    };
    use crate::plugins::payment::PaymentPluginRegistry;
    use crate::worker;

    let payment_plugins = PaymentPluginRegistry::from_env_with_pool(Some(pool.clone()));
    tracing::info!("payment plugins enabled: {:?}", payment_plugins.names());

    let users_state = UsersState {
        pool: pool.clone(),
        cfg: cfg.clone(),
    };

    let worker = worker::Worker::new(pool.clone(), cfg.clone(), 2);

    let static_service = ServeDir::new(&cfg.public_dir).append_index_html_on_directories(true);
    let hls_service = ServeDir::new(&cfg.media_dir);

    let static_router = Router::new()
        .route("/", get(|| async { Redirect::to("/public/") }))
        .route("/browse", get(|| async { Redirect::to("/public/") }))
        .route("/dashboard", get(|| async { Redirect::to("/public/dashboard.html") }))
        .route("/health", get(|| async { "ok" }))
        .nest_service("/public", static_service)
        .nest_service("/static_hls", hls_service);

    let admin_pages_router = Router::new()
        .route("/admin/data",                    get(admin_data))
        .route("/admin/payments",                get(admin_payments))
        .route("/admin/payments/:uid/disburse",  post(admin_disburse))
        .route("/admin/smtp",                    get(admin_smtp_get).post(admin_smtp_save))
        .with_state(AdminState { pool: pool.clone() });

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
        .route("/api/change_password", post(change_password))
        .with_state(AuthUserState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    let admin_auth_router = Router::new()
        .route(
            "/admin/login",
            get(|| async { Redirect::to("/public/admin/login.html") }).post(post_admin_login),
        )
        .route("/admin/logout", post(post_admin_logout))
        .route("/admin/change_password", post(admin_change_password))
        .with_state(AuthAdminState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    let setup_router = Router::new()
        .route("/setup_admin", get(setup_admin))
        .with_state(SetupState {
            pool: pool.clone(),
            token: std::env::var("ADMIN_BOOTSTRAP_TOKEN").ok(),
        });

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
        .with_state(VideoState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    let payment_plugin_router = Router::new()
        .route("/api/pay/providers", get(list_payment_plugins))
        .route("/api/pay/start", post(create_default_payment_invoice))
        .route("/api/pay/confirm", post(confirm_default_payment))
        .route("/api/pay/:provider/start", post(create_payment_invoice))
        .route("/api/pay/:provider/confirm", post(confirm_payment))
        .route("/api/pay/:provider/webhook", post(handle_webhook))
        .with_state(PaymentPluginState {
            registry: payment_plugins.clone(),
            pool:     pool.clone(),
            cfg:      cfg.clone(),
        });

    let users_router = Router::new()
        .route("/api/profile", get(get_my_profile))
        .route("/api/profile_update", post(update_my_profile))
        .route("/api/user_profile", get(public_profile))
        .with_state(users_state);

    let streaming_router = Router::new()
        .route("/api/request_play", get(request_play))
        .route("/hls/:session/:file", get(serve_hls))
        .with_state(StreamState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    let me_router = Router::new()
        .route("/api/me", get(me))
        .with_state(MeState {
            pool: pool.clone(),
            cfg: cfg.clone(),
        });

    let kurs_router = kurs_router(KursState { cfg: cfg.clone() });

    let app = static_router
        .merge(admin_pages_router)
        .merge(user_auth_router)
        .merge(admin_auth_router)
        .merge(setup_router)
        .merge(upload_router)
        .merge(video_router)
        .merge(payment_plugin_router)
        .merge(users_router)
        .merge(streaming_router)
        .merge(me_router)
        .merge(kurs_router)
        .layer(CookieManagerLayer::new());

    start_cleanup_task(pool.clone(), cfg.hls_root.clone());

    #[cfg(feature = "x402-watcher")]
    if std::env::var("WATCHER_ENABLE").ok().as_deref() == Some("1") {
        use crate::services::x402_watcher::run_watcher;
        use ethers::types::Address;

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

    let addr = cfg.bind.clone();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}
