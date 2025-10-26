// src/handlers/stream.rs
use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use tokio::fs;
use tower_cookies::Cookies;
use uuid::Uuid;
use std::path::{Path, PathBuf};
use tokio_util::io::ReaderStream;

use crate::config::Config;
use crate::handlers::video::user_has_view_access;
use crate::sessions;
use crate::ffmpeg::run_ffmpeg;

#[derive(Clone)]
pub struct StreamState {
    pub pool: PgPool,
    pub cfg: Config,
}

#[derive(Deserialize)]
pub struct RequestPlayQuery {
    pub video_id: String,
}

/// REQUEST_PLAY (VOD) + Watermark bergerak (CPU-only)
pub async fn request_play(
    State(st): State<StreamState>,
    cookies: Cookies,
    Query(q): Query<RequestPlayQuery>,
) -> impl IntoResponse {
    // 1) Auth
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "error": "not logged in"})),
            ).into_response()
        }
    };

    // 2) AuthZ
    match user_has_view_access(&st.pool, &q.video_id, &user_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"ok": false, "error": "no access"})),
            ).into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("db access error: {e}")})),
            ).into_response()
        }
    }

    // 3) Metadata video
    let row = match sqlx::query! {
        r#"
        SELECT id, owner_id, filename, title, price_cents
        FROM videos
        WHERE id = $1
        LIMIT 1
        "#,
        q.video_id
    }
    .fetch_optional(&st.pool)
    .await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("db error: {e}")})),
            ).into_response()
        }
    };
    let Some(v) = row else {
        return (StatusCode::NOT_FOUND, Json(json!({"ok": false, "error": "video not found"}))).into_response();
    };

    // 4) Username utk watermark
    let username: String = match sqlx::query_scalar! {
        r#"SELECT username FROM users WHERE id = $1 LIMIT 1"#,
        user_id
    }
    .fetch_one(&st.pool).await {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("user lookup: {e}")})),
            ).into_response()
        }
    };

    // 5) Siapkan session dir: hls_root/<session>
    if let Err(e) = fs::create_dir_all(&st.cfg.hls_root).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": format!("hls root: {e}")}))).into_response();
    }
    let session = Uuid::new_v4().to_string();
    let session_dir = Path::new(&st.cfg.hls_root).join(&session);
    if let Err(e) = fs::create_dir_all(&session_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": format!("mkdir session: {e}")}))).into_response();
    }

    // 6) Watermark textfile (strftime timestamp realtime)
    let wm_text = format!("• @{} • %Y-%m-%d %H\\:%M\\:%S", username);
    let wm_file = session_dir.join("wm.txt");
    if let Err(e) = fs::write(&wm_file, &wm_text).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": format!("write wm file: {e}")}))).into_response();
    }

    // 7) Filter watermark bergerak
    const MOVE_INTERVAL: i32 = 5;
    const PADDING: i32 = 20;
    let font_path = st.cfg.watermark_font.as_str();
    let seg_secs: u32 = st.cfg.hls_segment_seconds.max(3);

    let x_expr_raw = format!(
        "{pad} + (w-tw-{twopad})*mod(abs(sin(floor(t/{int})*12.9898)*43758.5453),1)",
        pad = PADDING, twopad = PADDING*2, int = MOVE_INTERVAL
    );
    let y_expr_raw = format!(
        "{pad} + (h-th-{twopad})*mod(abs(sin((floor(t/{int})+1)*78.233)*12345.6789),1)",
        pad = PADDING, twopad = PADDING*2, int = MOVE_INTERVAL
    );
    let x_expr = x_expr_raw.replace(",", "\\,");
    let y_expr = y_expr_raw.replace(",", "\\,");
    let wm_path = wm_file.to_string_lossy();

    let drawtext_shadow = format!(
        "drawtext=fontfile={font}:textfile={textfile}:expansion=strftime:x={x}+10:y={y}+10:fontsize=20:fontcolor=white@0.15:box=0",
        font = font_path, textfile = wm_path, x = x_expr, y = y_expr
    );
    let drawtext_main = format!(
        "drawtext=fontfile={font}:textfile={textfile}:expansion=strftime:x={x}:y={y}:fontsize=20:fontcolor=white:box=1:boxcolor=black@0.35:boxborderw=8",
        font = font_path, textfile = wm_path, x = x_expr, y = y_expr
    );
    let vf_chain = format!("{shadow},{main}", shadow = drawtext_shadow, main = drawtext_main);

    // 7a) Path input (HLS VOD siap > file asli)
    let input_path = match resolve_input_path(&st.cfg, &v.filename) {
        Some(p) => p,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "error": format!("source not found: {}", v.filename)}))).into_response();
        }
    };

    // 8) Transcode overlay → HLS event di folder session
    let master_rel = "master.m3u8".to_string();
    let args: Vec<String> = vec![
        "-hide_banner".into(), "-loglevel".into(), "error".into(),
        "-y".into(),
        "-i".into(), input_path.to_string_lossy().to_string(),
        "-vf".into(), vf_chain,
        "-c:v".into(), "libx264".into(),
        "-preset".into(), "veryfast".into(),
        "-profile:v".into(), "main".into(),
        "-level".into(), "4.0".into(),
        "-c:a".into(), "aac".into(),
        "-ac".into(), "2".into(),
        "-b:a".into(), "128k".into(),
        "-threads".into(), format!("{}", num_cpus::get().max(2)),
        "-start_number".into(), "0".into(),
        "-hls_time".into(), seg_secs.to_string(),
        "-hls_playlist_type".into(), "event".into(),
        "-hls_flags".into(), "independent_segments+delete_segments".into(),
        "-hls_list_size".into(), "0".into(),
        master_rel.clone(),
    ];

    if let Err(e) = run_ffmpeg(&args, &session_dir.to_string_lossy()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("ffmpeg: {e}")}))
        ).into_response();
    }

    // 9) Response
    let playlist = format!("/hls/{}/master.m3u8", session);
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "session": session,
            "playlist": playlist,
            "video_id": v.id,
            "title": v.title,
            "price_cents": v.price_cents,
            "segment_seconds": seg_secs
        })),
    ).into_response()
}

/// Prefer hasil transcode worker: media_dir/<video_id>/master.m3u8 (HLS siap)
/// Kalau tidak ada, pakai file asli (uploads / storage_dir / media_dir / cwd)
fn resolve_input_path(cfg: &Config, filename_in_db: &str) -> Option<PathBuf> {
    let p = PathBuf::from(filename_in_db);
    if p.is_absolute() && p.exists() { return Some(p); }

    if !cfg.upload_dir.trim().is_empty() {
        let mut u = PathBuf::from(&cfg.upload_dir);
        u.push(filename_in_db);
        if u.exists() { return Some(u); }
    }
    let mut cand = PathBuf::from(&cfg.media_dir);
    cand.push(filename_in_db);
    if cand.exists() { return Some(cand); }

    let mut up = PathBuf::from("uploads");
    up.push(filename_in_db);
    if up.exists() { return Some(up); }

    if Path::new(filename_in_db).exists() { return Some(PathBuf::from(filename_in_db)); }
    None
}

/// Stream file HLS dari hls_root/<session>/... (no-store)
pub async fn serve_hls(
    State(st): State<StreamState>,
    AxumPath((session, file)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    if !is_safe_token(&session) || !is_safe_file(&file) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }
    let fp = Path::new(&st.cfg.hls_root).join(&session).join(&file);
    match fs::File::open(&fp).await {
        Ok(fh) => {
            // Content-Type berdasarkan NAMA FILE (bukan handle)
            let ctype = match file_type(&file) {
                FileType::M3U8 => "application/vnd.apple.mpegurl",
                FileType::TS   => "video/mp2t",
                FileType::M4S  => "video/iso.segment",
                FileType::MP4  => "video/mp4",
                FileType::Unknown => "application/octet-stream",
            };
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

            let stream = ReaderStream::new(fh);
            (StatusCode::OK, headers, axum::body::Body::from_stream(stream)).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "segment not found").into_response(),
    }
}

#[derive(Debug, Clone, Copy)]
enum FileType { M3U8, TS, M4S, MP4, Unknown }

fn file_type(name: &str) -> FileType {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".m3u8") { FileType::M3U8 }
    else if lower.ends_with(".ts") { FileType::TS }
    else if lower.ends_with(".m4s") { FileType::M4S }
    else if lower.ends_with(".mp4") { FileType::MP4 }
    else { FileType::Unknown }
}

fn is_safe_token(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c=='-' || c=='_')
}
fn is_safe_file(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('/') && !s.contains('\\') && !s.contains("..")
        && (s.ends_with(".m3u8") || s.ends_with(".ts") || s.ends_with(".m4s") || s.ends_with(".mp4"))
}
