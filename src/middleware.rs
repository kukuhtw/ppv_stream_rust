use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::{
        header::{CACHE_CONTROL, ORIGIN, REFERER},
        HeaderValue, Method, StatusCode, Uri,
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use once_cell::sync::Lazy;

use crate::config::Config;

static RATE_LIMIT_STORE: Lazy<Mutex<HashMap<String, VecDeque<Instant>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(300);
const RATE_LIMIT_MAX_ATTEMPTS: usize = 20;

pub async fn browser_csrf_guard(
    cfg: Config,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    if should_enforce_same_origin(&req) {
        let headers = req.headers();
        let origin_ok = headers
            .get(ORIGIN)
            .and_then(|value| value.to_str().ok())
            .map(|value| same_origin(value, &cfg.base_url))
            .unwrap_or(false);
        let referer_ok = headers
            .get(REFERER)
            .and_then(|value| value.to_str().ok())
            .map(|value| same_origin(value, &cfg.base_url))
            .unwrap_or(false);

        if !origin_ok && !referer_ok {
            tracing::warn!(
                path = %req.uri().path(),
                method = %req.method(),
                "blocked cross-origin state-changing request"
            );
            return Err((
                StatusCode::FORBIDDEN,
                "cross-origin state-changing request blocked",
            )
                .into_response());
        }
    }

    Ok(next.run(req).await)
}

pub async fn basic_rate_limit(cfg: Config, req: Request, next: Next) -> Result<Response, Response> {
    if should_rate_limit(req.uri(), req.method()) {
        let client_key = client_fingerprint(&req, cfg.trust_proxy_headers);
        let key = format!("{}:{}", req.uri().path(), client_key);
        let now = Instant::now();

        let mut store = RATE_LIMIT_STORE.lock().expect("rate limit mutex poisoned");
        let bucket = store.entry(key).or_default();
        while let Some(ts) = bucket.front() {
            if now.duration_since(*ts) > RATE_LIMIT_WINDOW {
                bucket.pop_front();
            } else {
                break;
            }
        }

        if bucket.len() >= RATE_LIMIT_MAX_ATTEMPTS {
            tracing::warn!(
                path = %req.uri().path(),
                client = %client_key,
                "rate limit exceeded"
            );
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests, please retry later",
            )
                .into_response());
        }

        bucket.push_back(now);
    }

    Ok(next.run(req).await)
}

pub async fn security_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );

    if !headers.contains_key(CACHE_CONTROL) && should_default_no_store(&path) {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }

    response
}

fn should_enforce_same_origin(req: &Request) -> bool {
    matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) && has_session_cookie(req)
        && !req.uri().path().starts_with("/api/pay/")
}

fn has_session_cookie(req: &Request) -> bool {
    req.headers()
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.contains("ppv_session="))
        .unwrap_or(false)
}

fn same_origin(candidate: &str, base_url: &str) -> bool {
    let normalize = |value: &str| value.trim().trim_end_matches('/').to_ascii_lowercase();
    let expected = normalize(base_url);
    let current = normalize(candidate);
    current == expected || current.starts_with(&(expected + "/"))
}

fn should_rate_limit(uri: &Uri, method: &Method) -> bool {
    if *method != Method::POST {
        return false;
    }

    matches!(
        uri.path(),
        "/auth/login"
            | "/admin/login"
            | "/auth/register"
            | "/auth/forgot"
            | "/setup_admin"
            | "/api/change_password"
            | "/admin/change_password"
    )
}

fn should_default_no_store(path: &str) -> bool {
    !(path.starts_with("/public/")
        || path.starts_with("/static_hls/")
        || path == "/"
        || path == "/browse"
        || path == "/dashboard"
        || path == "/health")
}

fn client_fingerprint(req: &Request, trust_proxy_headers: bool) -> String {
    if trust_proxy_headers {
        if let Some(value) = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
        {
            let ip = value.split(',').next().unwrap_or("").trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }

        if let Some(value) = req
            .headers()
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
        {
            if !value.trim().is_empty() {
                return value.trim().to_string();
            }
        }
    }

    req.headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("ua:{value}"))
        .unwrap_or_else(|| "unknown".to_string())
}
