// src/handlers/stream.rs
//
// Playback session and HLS delivery handlers.

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{fs, sync::oneshot, time::sleep};
use tokio_util::io::ReaderStream;
use tower_cookies::Cookies;
use tracing::{error, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::ffmpeg::run_ffmpeg;
use crate::handlers::video::user_has_view_access;
use crate::sessions;

const PLAYBACK_SESSION_TTL_SECONDS: i32 = 60 * 60;
const PLAYLIST_READY_TIMEOUT_SECONDS: u64 = 30;
const PLAYBACK_HLS_LIST_SIZE: u32 = 8;

#[derive(Clone)]
pub struct StreamState {
    pub pool: PgPool,
    pub cfg: Config,
}

#[derive(Deserialize)]
pub struct RequestPlayQuery {
    pub video_id: String,
}

pub async fn request_play(
    State(st): State<StreamState>,
    cookies: Cookies,
    Query(q): Query<RequestPlayQuery>,
) -> impl IntoResponse {
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(value) => value,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "error": "not logged in"})),
            )
                .into_response()
        }
    };

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

    let row = match sqlx::query!(
        r#"
        SELECT id, owner_id, filename, title, price_cents
        FROM videos
        WHERE id = $1
        LIMIT 1
        "#,
        q.video_id
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("db error: {e}")})),
            )
                .into_response()
        }
    };

    let Some(video) = row else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"ok": false, "error": "video not found"})),
        )
            .into_response();
    };

    let username: String = match sqlx::query_scalar!(
        r#"SELECT username FROM users WHERE id = $1 LIMIT 1"#,
        user_id
    )
    .fetch_one(&st.pool)
    .await
    {
        Ok(username) => username,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("user lookup: {e}")})),
            )
                .into_response()
        }
    };

    if let Err(e) = cleanup_expired_sessions(&st.pool, &st.cfg.hls_root).await {
        warn!("playback session cleanup failed: {e}");
    }

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

    let safe_username: String = username
        .chars()
        .filter(|character| !character.is_control())
        .take(80)
        .collect();
    let watermark_text = format!("• @{} • %Y-%m-%d %H\\:%M\\:%S", safe_username);
    let watermark_file = session_dir.join("wm.txt");

    if let Err(e) = fs::write(&watermark_file, &watermark_text).await {
        let _ = fs::remove_dir_all(&session_dir).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("write wm file: {e}")})),
        )
            .into_response();
    }

    let session_dir_string = session_dir.to_string_lossy().to_string();
    if let Err(e) = sqlx::query!(
        r#"
        INSERT INTO playback_sessions
            (session_id, user_id, video_id, session_dir, status, expires_at)
        VALUES
            ($1, $2, $3, $4, 'starting', NOW() + make_interval(secs => $5))
        "#,
        session,
        user_id,
        video.id,
        session_dir_string,
        PLAYBACK_SESSION_TTL_SECONDS
    )
    .execute(&st.pool)
    .await
    {
        let _ = fs::remove_dir_all(&session_dir).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": format!("create playback session: {e}")})),
        )
            .into_response();
    }

    const MOVE_INTERVAL: i32 = 5;
    const PADDING: i32 = 20;

    let font_path = st.cfg.watermark_font.as_str();
    let segment_seconds = st.cfg.hls_segment_seconds.max(3);

    let x_expression_raw = format!(
        "{pad} + (w-tw-{twopad})*mod(abs(sin(floor(t/{interval})*12.9898)*43758.5453),1)",
        pad = PADDING,
        twopad = PADDING * 2,
        interval = MOVE_INTERVAL
    );
    let y_expression_raw = format!(
        "{pad} + (h-th-{twopad})*mod(abs(sin((floor(t/{interval})+1)*78.233)*12345.6789),1)",
        pad = PADDING,
        twopad = PADDING * 2,
        interval = MOVE_INTERVAL
    );

    let x_expression = x_expression_raw.replace(",", "\\,");
    let y_expression = y_expression_raw.replace(",", "\\,");
    let watermark_path = watermark_file.to_string_lossy();

    let drawtext_shadow = format!(
        "drawtext=fontfile={font}:textfile={textfile}:expansion=strftime:x={x}+10:y={y}+10:fontsize=20:fontcolor=white@0.15:box=0",
        font = font_path,
        textfile = watermark_path,
        x = x_expression,
        y = y_expression
    );
    let drawtext_main = format!(
        "drawtext=fontfile={font}:textfile={textfile}:expansion=strftime:x={x}:y={y}:fontsize=20:fontcolor=white:box=1:boxcolor=black@0.35:boxborderw=8",
        font = font_path,
        textfile = watermark_path,
        x = x_expression,
        y = y_expression
    );
    let video_filter = format!("{drawtext_shadow},{drawtext_main}");

    let input_path = match resolve_input_path(&st.cfg, &video.filename) {
        Some(path) => path,
        None => {
            let _ = mark_session_failed(&st.pool, &session, "source file not found").await;
            let _ = fs::remove_dir_all(&session_dir).await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "ok": false,
                    "error": format!("source not found: {}", video.filename)
                })),
            )
                .into_response();
        }
    };

    let playlist_name = "master.m3u8".to_string();
    let arguments: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input_path.to_string_lossy().to_string(),
        "-vf".into(),
        video_filter,
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
        format!("{}", num_cpus::get().min(4).max(1)),
        "-start_number".into(),
        "0".into(),
        "-hls_time".into(),
        segment_seconds.to_string(),
        "-hls_flags".into(),
        "independent_segments+delete_segments+append_list".into(),
        "-hls_list_size".into(),
        PLAYBACK_HLS_LIST_SIZE.to_string(),
        "-hls_delete_threshold".into(),
        "2".into(),
        "-hls_segment_filename".into(),
        "segment_%06d.ts".into(),
        playlist_name.clone(),
    ];

    let (status_tx, mut status_rx) = oneshot::channel::<Result<(), String>>();
    let pool_for_task = st.pool.clone();
    let session_for_task = session.clone();
    let session_dir_for_task = session_dir.clone();

    let ffmpeg_task = tokio::spawn(async move {
        let result = run_ffmpeg(&arguments, &session_dir_for_task.to_string_lossy()).await;
        match result {
            Ok(()) => {
                let _ = sqlx::query!(
                    "UPDATE playback_sessions SET status='completed', last_error=NULL WHERE session_id=$1",
                    session_for_task
                )
                .execute(&pool_for_task)
                .await;
                let _ = status_tx.send(Ok(()));
            }
            Err(e) => {
                let message = e.to_string();
                error!(
                    "playback ffmpeg failed for session {}: {}",
                    session_for_task, message
                );
                let _ = mark_session_failed(&pool_for_task, &session_for_task, &message).await;
                let _ = fs::remove_dir_all(&session_dir_for_task).await;
                let _ = status_tx.send(Err(message));
            }
        }
    });

    let playlist_path = session_dir.join(&playlist_name);
    let startup_timeout = sleep(Duration::from_secs(PLAYLIST_READY_TIMEOUT_SECONDS));
    tokio::pin!(startup_timeout);

    loop {
        if playlist_path.exists() {
            if let Err(e) = sqlx::query!(
                "UPDATE playback_sessions SET status='ready', last_error=NULL WHERE session_id=$1",
                session
            )
            .execute(&st.pool)
            .await
            {
                ffmpeg_task.abort();
                let message = format!("mark playlist ready: {e}");
                let _ = mark_session_failed(&st.pool, &session, &message).await;
                let _ = fs::remove_dir_all(&session_dir).await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"ok": false, "error": message})),
                )
                    .into_response();
            }
            break;
        }

        tokio::select! {
            result = &mut status_rx => {
                match result {
                    Ok(Err(message)) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"ok": false, "error": format!("ffmpeg: {message}")})),
                        ).into_response();
                    }
                    Ok(Ok(())) => {
                        if !playlist_path.exists() {
                            let message = "ffmpeg completed without creating a playlist";
                            let _ = mark_session_failed(&st.pool, &session, message).await;
                            let _ = fs::remove_dir_all(&session_dir).await;
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(json!({"ok": false, "error": message})),
                            ).into_response();
                        }
                    }
                    Err(_) => {
                        let message = "ffmpeg status channel closed";
                        let _ = mark_session_failed(&st.pool, &session, message).await;
                        let _ = fs::remove_dir_all(&session_dir).await;
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"ok": false, "error": message})),
                        ).into_response();
                    }
                }
            }
            _ = &mut startup_timeout => {
                ffmpeg_task.abort();
                let _ = mark_session_failed(&st.pool, &session, "playlist startup timeout").await;
                let _ = fs::remove_dir_all(&session_dir).await;
                return (
                    StatusCode::GATEWAY_TIMEOUT,
                    Json(json!({
                        "ok": false,
                        "error": "playlist was not generated within the startup timeout"
                    })),
                ).into_response();
            }
            _ = sleep(Duration::from_millis(200)) => {}
        }
    }

    let playlist = format!("/hls/{}/master.m3u8", session);
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "session": session,
            "playlist": playlist,
            "video_id": video.id,
            "title": video.title,
            "price_cents": video.price_cents,
            "segment_seconds": segment_seconds,
            "expires_in_seconds": PLAYBACK_SESSION_TTL_SECONDS
        })),
    )
        .into_response()
}

fn resolve_input_path(cfg: &Config, filename_in_db: &str) -> Option<PathBuf> {
    let path = PathBuf::from(filename_in_db);

    if path.is_absolute() && path.exists() {
        return Some(path);
    }

    if !cfg.upload_dir.trim().is_empty() {
        let candidate = PathBuf::from(&cfg.upload_dir).join(filename_in_db);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let media_candidate = PathBuf::from(&cfg.media_dir).join(filename_in_db);
    if media_candidate.exists() {
        return Some(media_candidate);
    }

    let legacy_candidate = PathBuf::from("uploads").join(filename_in_db);
    if legacy_candidate.exists() {
        return Some(legacy_candidate);
    }

    if Path::new(filename_in_db).exists() {
        return Some(PathBuf::from(filename_in_db));
    }

    None
}

async fn mark_session_failed(
    pool: &PgPool,
    session_id: &str,
    message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE playback_sessions SET status='error', last_error=$2 WHERE session_id=$1",
        session_id,
        message
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn cleanup_expired_sessions(pool: &PgPool, hls_root: &str) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT session_id, session_dir
        FROM playback_sessions
        WHERE expires_at <= NOW()
        "#
    )
    .fetch_all(pool)
    .await?;

    let mut removed = 0u64;
    for row in rows {
        let path = if row.session_dir.trim().is_empty() {
            Path::new(hls_root).join(&row.session_id)
        } else {
            PathBuf::from(&row.session_dir)
        };

        let directory_removed = match fs::remove_dir_all(&path).await {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => {
                warn!(
                    "failed to delete expired playback directory {}: {}",
                    path.display(),
                    e
                );
                false
            }
        };

        if directory_removed {
            sqlx::query!(
                "DELETE FROM playback_sessions WHERE session_id=$1",
                row.session_id
            )
            .execute(pool)
            .await?;
            removed += 1;
        }
    }

    Ok(removed)
}

pub fn start_cleanup_task(pool: PgPool, hls_root: String) {
    tokio::spawn(async move {
        loop {
            match cleanup_expired_sessions(&pool, &hls_root).await {
                Ok(count) if count > 0 => {
                    tracing::info!("removed {} expired playback sessions", count);
                }
                Ok(_) => {}
                Err(e) => warn!("periodic playback cleanup failed: {e}"),
            }
            sleep(Duration::from_secs(300)).await;
        }
    });
}

pub async fn serve_hls(
    State(st): State<StreamState>,
    cookies: Cookies,
    AxumPath((session, file)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    if !is_safe_token(&session) || !is_safe_file(&file) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let (current_user_id, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(value) => value,
        None => return (StatusCode::UNAUTHORIZED, "not logged in").into_response(),
    };

    let session_row = match sqlx::query!(
        r#"
        SELECT user_id, session_dir, status, expires_at > NOW() AS "is_active!"
        FROM playback_sessions
        WHERE session_id=$1
        LIMIT 1
        "#,
        session
    )
    .fetch_optional(&st.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "session lookup failed").into_response()
        }
    };

    let Some(session_row) = session_row else {
        return (StatusCode::GONE, "playback session expired").into_response();
    };

    if !session_row.is_active {
        let _ = sqlx::query!("DELETE FROM playback_sessions WHERE session_id=$1", session)
            .execute(&st.pool)
            .await;
        let _ = fs::remove_dir_all(&session_row.session_dir).await;
        return (StatusCode::GONE, "playback session expired").into_response();
    }

    if session_row.user_id != current_user_id {
        return (
            StatusCode::FORBIDDEN,
            "session does not belong to this user",
        )
            .into_response();
    }

    if session_row.status == "error" {
        return (StatusCode::GONE, "playback session failed").into_response();
    }

    let root = match fs::canonicalize(&st.cfg.hls_root).await {
        Ok(path) => path,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "hls root is unavailable").into_response()
        }
    };
    let session_dir = match fs::canonicalize(&session_row.session_dir).await {
        Ok(path) => path,
        Err(_) => {
            return (StatusCode::GONE, "playback session files are unavailable").into_response()
        }
    };

    if !session_dir.starts_with(&root) {
        return (StatusCode::FORBIDDEN, "invalid playback session path").into_response();
    }

    let file_path = session_dir.join(&file);

    match fs::File::open(&file_path).await {
        Ok(file_handle) => {
            let content_type = match file_type(&file) {
                FileType::M3U8 => "application/vnd.apple.mpegurl",
                FileType::TS => "video/mp2t",
                FileType::M4S => "video/iso.segment",
                FileType::MP4 => "video/mp4",
                FileType::Unknown => "application/octet-stream",
            };

            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

            let stream = ReaderStream::new(file_handle);
            (
                StatusCode::OK,
                headers,
                axum::body::Body::from_stream(stream),
            )
                .into_response()
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "segment not found").into_response()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "segment read failed").into_response(),
    }
}

#[derive(Debug, Clone, Copy)]
enum FileType {
    M3U8,
    TS,
    M4S,
    MP4,
    Unknown,
}

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

fn is_safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

fn is_safe_file(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains("..")
        && (lower.ends_with(".m3u8")
            || lower.ends_with(".ts")
            || lower.ends_with(".m4s")
            || lower.ends_with(".mp4"))
}
