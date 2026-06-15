// src/handlers/stream.rs
//
// Playback session and HLS delivery handlers.
//
// This module authenticates viewers, checks video access rights, creates a
// viewer-specific HLS session, applies a moving watermark with FFmpeg, and
// streams generated HLS playlists and media segments back to the client.

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio_util::io::ReaderStream;
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::config::Config;
use crate::ffmpeg::run_ffmpeg;
use crate::handlers::video::user_has_view_access;
use crate::sessions;

/// Shared state used by playback and HLS file handlers.
///
/// `pool` provides database access for authentication and authorization.
/// `cfg` provides storage paths, watermark configuration, and HLS settings.
#[derive(Clone)]
pub struct StreamState {
    pub pool: PgPool,
    pub cfg: Config,
}

/// Query parameters accepted by `GET /api/request_play`.
#[derive(Deserialize)]
pub struct RequestPlayQuery {
    pub video_id: String,
}

/// Creates a watermarked playback session for one authenticated viewer.
///
/// Flow:
/// 1. Authenticate the current user.
/// 2. Verify ownership or allowlist access.
/// 3. Load video and viewer metadata.
/// 4. Create a unique session directory.
/// 5. Build a moving username and timestamp watermark.
/// 6. Generate HLS output with FFmpeg.
/// 7. Return the session playlist URL.
pub async fn request_play(
    State(st): State<StreamState>,
    cookies: Cookies,
    Query(q): Query<RequestPlayQuery>,
) -> impl IntoResponse {
    // Step 1: Authenticate the viewer using the signed session cookie and the
    // server-side sessions table. Anonymous playback is not allowed.
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "error": "not logged in"})),
            )
                .into_response()
        }
    };

    // Step 2: Authorize playback. Access is granted when the viewer owns the
    // video or is present in the allowlist. Successful purchases also create
    // allowlist records, linking payment completion to playback access.
    match user_has_view_access(&st.pool, &q.video_id, &user_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"ok": false, "error": "no access"})),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("db access error: {e}")})),
            )
                .into_response()
        }
    }

    // Step 3: Load the source filename and display metadata for the requested
    // video. `fetch_optional` distinguishes a missing record from a query error.
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
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("db error: {e}")})),
            )
                .into_response()
        }
    };

    // Stop immediately when the requested video does not exist.
    let Some(v) = row else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"ok": false, "error": "video not found"})),
        )
            .into_response();
    };

    // Step 4: Load the viewer username. The username becomes part of the
    // watermark so leaked recordings can be associated with the viewer.
    let username: String = match sqlx::query_scalar! {
        r#"SELECT username FROM users WHERE id = $1 LIMIT 1"#,
        user_id
    }
    .fetch_one(&st.pool)
    .await
    {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("user lookup: {e}")})),
            )
                .into_response()
        }
    };

    // Step 5: Ensure the HLS session root exists, then create a unique session
    // directory for this playback request.
    if let Err(e) = fs::create_dir_all(&st.cfg.hls_root).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("hls root: {e}")})),
        )
            .into_response();
    }

    let session = Uuid::new_v4().to_string();
    let session_dir = Path::new(&st.cfg.hls_root).join(&session);

    if let Err(e) = fs::create_dir_all(&session_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("mkdir session: {e}")})),
        )
            .into_response();
    }

    // Step 6: Write watermark text into a file consumed by FFmpeg drawtext.
    // FFmpeg expands the date and time template during encoding.
    let wm_text = format!("• @{} • %Y-%m-%d %H\\:%M\\:%S", username);
    let wm_file = session_dir.join("wm.txt");

    if let Err(e) = fs::write(&wm_file, &wm_text).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("write wm file: {e}")})),
        )
            .into_response();
    }

    // Step 7: Build a watermark position that changes every few seconds.
    // A moving watermark is harder to remove through simple cropping or masking.
    const MOVE_INTERVAL: i32 = 5;
    const PADDING: i32 = 20;

    let font_path = st.cfg.watermark_font.as_str();
    let seg_secs: u32 = st.cfg.hls_segment_seconds.max(3);

    // Generate deterministic pseudo-random X and Y positions using FFmpeg
    // expressions based on playback time and video dimensions.
    let x_expr_raw = format!(
        "{pad} + (w-tw-{twopad})*mod(abs(sin(floor(t/{int})*12.9898)*43758.5453),1)",
        pad = PADDING,
        twopad = PADDING * 2,
        int = MOVE_INTERVAL
    );
    let y_expr_raw = format!(
        "{pad} + (h-th-{twopad})*mod(abs(sin((floor(t/{int})+1)*78.233)*12345.6789),1)",
        pad = PADDING,
        twopad = PADDING * 2,
        int = MOVE_INTERVAL
    );

    // Escape commas because commas separate filters in FFmpeg syntax.
    let x_expr = x_expr_raw.replace(",", "\\,");
    let y_expr = y_expr_raw.replace(",", "\\,");
    let wm_path = wm_file.to_string_lossy();

    // Draw a low-opacity offset shadow to keep the watermark readable over
    // both bright and dark scenes.
    let drawtext_shadow = format!(
        "drawtext=fontfile={font}:textfile={textfile}:expansion=strftime:x={x}+10:y={y}+10:fontsize=20:fontcolor=white@0.15:box=0",
        font = font_path,
        textfile = wm_path,
        x = x_expr,
        y = y_expr
    );

    // Draw the main watermark with a semi-transparent dark background box.
    let drawtext_main = format!(
        "drawtext=fontfile={font}:textfile={textfile}:expansion=strftime:x={x}:y={y}:fontsize=20:fontcolor=white:box=1:boxcolor=black@0.35:boxborderw=8",
        font = font_path,
        textfile = wm_path,
        x = x_expr,
        y = y_expr
    );

    // Apply both drawtext filters sequentially.
    let vf_chain = format!(
        "{shadow},{main}",
        shadow = drawtext_shadow,
        main = drawtext_main
    );

    // Step 8: Resolve the source media path from configured and legacy storage
    // locations. Playback cannot continue when the source file is missing.
    let input_path = match resolve_input_path(&st.cfg, &v.filename) {
        Some(p) => p,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "ok": false,
                    "error": format!("source not found: {}", v.filename)
                })),
            )
                .into_response();
        }
    };

    // Step 9: Build the FFmpeg command for one HLS rendition. The output file
    // is named `master.m3u8`, although this command currently creates a single
    // media playlist rather than a multi-variant adaptive bitrate master list.
    let master_rel = "master.m3u8".to_string();
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input_path.to_string_lossy().to_string(),
        "-vf".into(),
        vf_chain,
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-profile:v".into(),
        "main".into(),
        "-level".into(),
        "4.0".into(),
        "-c:a".into(),
        "aac".into(),
        "-ac".into(),
        "2".into(),
        "-b:a".into(),
        "128k".into(),
        "-threads".into(),
        format!("{}", num_cpus::get().max(2)),
        "-start_number".into(),
        "0".into(),
        "-hls_time".into(),
        seg_secs.to_string(),
        "-hls_playlist_type".into(),
        "event".into(),
        "-hls_flags".into(),
        "independent_segments+delete_segments".into(),
        "-hls_list_size".into(),
        "0".into(),
        master_rel.clone(),
    ];

    // Run FFmpeg inside the session directory so the generated playlist and
    // segments remain isolated under this playback session UUID.
    if let Err(e) = run_ffmpeg(&args, &session_dir.to_string_lossy()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("ffmpeg: {e}")})),
        )
            .into_response();
    }

    // Step 10: Return the playlist URL and playback metadata to the frontend.
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
    )
        .into_response()
}

/// Resolves a database filename into an existing media source path.
///
/// Search order:
/// 1. Existing absolute path.
/// 2. Configured upload directory.
/// 3. Configured media directory.
/// 4. Legacy `uploads` directory.
/// 5. Current working directory.
fn resolve_input_path(cfg: &Config, filename_in_db: &str) -> Option<PathBuf> {
    let path = PathBuf::from(filename_in_db);

    if path.is_absolute() && path.exists() {
        return Some(path);
    }

    if !cfg.upload_dir.trim().is_empty() {
        let mut upload_candidate = PathBuf::from(&cfg.upload_dir);
        upload_candidate.push(filename_in_db);
        if upload_candidate.exists() {
            return Some(upload_candidate);
        }
    }

    let mut media_candidate = PathBuf::from(&cfg.media_dir);
    media_candidate.push(filename_in_db);
    if media_candidate.exists() {
        return Some(media_candidate);
    }

    let mut legacy_upload_candidate = PathBuf::from("uploads");
    legacy_upload_candidate.push(filename_in_db);
    if legacy_upload_candidate.exists() {
        return Some(legacy_upload_candidate);
    }

    if Path::new(filename_in_db).exists() {
        return Some(PathBuf::from(filename_in_db));
    }

    None
}

/// Streams one HLS playlist or segment from a playback session directory.
///
/// Route format: `GET /hls/:session/:file`
///
/// The function validates both path components, opens the file asynchronously,
/// applies the correct content type, disables caching, and streams bytes without
/// loading the complete file into memory.
pub async fn serve_hls(
    State(st): State<StreamState>,
    AxumPath((session, file)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    // Reject unsafe path components before joining them with the HLS root.
    if !is_safe_token(&session) || !is_safe_file(&file) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let file_path = Path::new(&st.cfg.hls_root).join(&session).join(&file);

    match fs::File::open(&file_path).await {
        Ok(file_handle) => {
            // Determine the response MIME type from the filename extension.
            let content_type = match file_type(&file) {
                FileType::M3U8 => "application/vnd.apple.mpegurl",
                FileType::TS => "video/mp2t",
                FileType::M4S => "video/iso.segment",
                FileType::MP4 => "video/mp4",
                FileType::Unknown => "application/octet-stream",
            };

            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type),
            );

            // Session-specific watermarked output should not be retained by
            // browsers or intermediary caches.
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store"),
            );

            // Convert the asynchronous file handle into a streaming HTTP body.
            let stream = ReaderStream::new(file_handle);
            (
                StatusCode::OK,
                headers,
                axum::body::Body::from_stream(stream),
            )
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "segment not found").into_response(),
    }
}

/// Media file types supported by the HLS delivery endpoint.
#[derive(Debug, Clone, Copy)]
enum FileType {
    M3U8,
    TS,
    M4S,
    MP4,
    Unknown,
}

/// Classifies a requested filename so the correct HTTP content type can be used.
fn file_type(name: &str) -> FileType {
    let lower = name.to_ascii_lowercase();

    if lower.ends_with(".m3u8") {
        FileType::M3U8
    } else if lower.ends_with(".ts") {
        FileType::TS
    } else if lower.ends_with(".m4s") {
        FileType::M4S
    } else if lower.ends_with(".mp4") {
        FileType::MP4
    } else {
        FileType::Unknown
    }
}

/// Validates a playback session identifier.
///
/// UUID session IDs contain alphanumeric characters and hyphens. Underscores
/// are also accepted for compatibility with possible future token formats.
fn is_safe_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Validates one HLS filename and prevents path traversal.
///
/// Directory separators and parent-directory markers are rejected. Only known
/// playlist and media file extensions are permitted.
fn is_safe_file(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && (value.ends_with(".m3u8")
            || value.ends_with(".ts")
            || value.ends_with(".m4s")
            || value.ends_with(".mp4"))
}
