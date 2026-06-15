// src/handlers/upload.rs
//
// Video upload endpoint and upload-to-transcoding handoff.
//
// This module is responsible for:
// 1. Authenticating the uploader.
// 2. Reading multipart form fields without loading the whole video into memory.
// 3. Validating file extension, size, MIME type, title, and price.
// 4. Rejecting requests that contain more than one video file.
// 5. Writing the upload safely through a temporary `.part` file.
// 6. Inserting the video metadata into PostgreSQL.
// 7. Enqueuing a background transcoding job.

use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use bytesize::ByteSize;
use serde_json::json;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    fs::File,
    io::{AsyncWriteExt, BufWriter},
};
use tower_cookies::Cookies;
use tracing::info;
use uuid::Uuid;

use crate::{
    config::Config,
    sessions,
    worker::{TranscodeJob, Worker},
};

const MAX_TITLE_CHARS: usize = 200;

#[derive(Clone)]
pub struct UploadState {
    pub cfg: Config,
    pub pool: PgPool,
    pub worker: Worker,
}

pub async fn upload_video(
    State(st): State<UploadState>,
    cookies: Cookies,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "where": "auth", "error": "not logged in"})),
            )
                .into_response()
        }
    };

    let upload_dir = if !st.cfg.upload_dir.is_empty() {
        st.cfg.upload_dir.clone()
    } else {
        st.cfg.storage_dir.clone()
    };

    if let Err(e) = fs::create_dir_all(&upload_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "where": "mkdir_upload", "error": e.to_string()})),
        )
            .into_response();
    }

    let mut title = String::from("Untitled");
    let mut price_cents: i64 = 0;
    let mut file_count: u32 = 0;
    let video_id = Uuid::new_v4().to_string();
    let mut saved_path: Option<PathBuf> = None;
    let mut saved_filename_only: Option<String> = None;
    let mut total_bytes: u64 = 0;
    let max_bytes = st.cfg.max_upload_bytes;
    let allowed_extensions: Vec<String> = st
        .cfg
        .allow_exts
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();

    while let Some(field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "where": "multipart", "error": e.to_string()})),
            )
                .into_response()
        }
    } {
        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            "title" => {
                title = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "Untitled".into())
                    .trim()
                    .to_string();
            }
            "price_cents" => {
                price_cents = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "0".into())
                    .trim()
                    .parse()
                    .unwrap_or(0);
            }
            "file" => {
                file_count += 1;
                if file_count > 1 {
                    if let Some(path) = saved_path.as_ref() {
                        let _ = fs::remove_file(path).await;
                    }
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "ok": false,
                            "where": "validation",
                            "error": "only one file field is allowed"
                        })),
                    )
                        .into_response();
                }

                let mut file_field = field;
                let extension = file_field
                    .file_name()
                    .and_then(|name| Path::new(name).extension().and_then(|ext| ext.to_str()))
                    .map(|ext| ext.to_ascii_lowercase())
                    .filter(|ext| !ext.is_empty())
                    .unwrap_or_else(|| "mp4".to_string());

                if !allowed_extensions.contains(&extension) {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "ok": false,
                            "where": "validation",
                            "error": format!("file extension not allowed: .{}", extension)
                        })),
                    )
                        .into_response();
                }

                let filename = format!("{}.{}", &video_id, extension);
                let full_path = Path::new(&upload_dir).join(&filename);
                let temporary_path = full_path.with_extension(format!("{}.part", extension));
                let temporary_dir = temporary_path
                    .parent()
                    .unwrap_or_else(|| Path::new(&upload_dir))
                    .to_path_buf();

                if let Err(e) = fs::create_dir_all(&temporary_dir).await {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"ok": false, "where": "mkdir_temp", "error": e.to_string()})),
                    )
                        .into_response();
                }

                let output_file = match File::create(&temporary_path).await {
                    Ok(file_handle) => file_handle,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"ok": false, "where": "create_file", "error": e.to_string()})),
                        )
                            .into_response()
                    }
                };

                let mut output = BufWriter::with_capacity(1024 * 1024, output_file);
                let mut first_sniff: Option<Vec<u8>> = None;

                while let Some(chunk_result) = file_field.chunk().await.transpose() {
                    match chunk_result {
                        Ok(bytes) => {
                            total_bytes += bytes.len() as u64;
                            if total_bytes > max_bytes {
                                let _ = fs::remove_file(&temporary_path).await;
                                return (
                                    StatusCode::PAYLOAD_TOO_LARGE,
                                    Json(json!({
                                        "ok": false,
                                        "where": "validation",
                                        "error": format!(
                                            "file too large: {} > {}",
                                            ByteSize(total_bytes),
                                            ByteSize(max_bytes)
                                        )
                                    })),
                                )
                                    .into_response();
                            }

                            if first_sniff.is_none() {
                                first_sniff = Some(bytes[..bytes.len().min(8192)].to_vec());
                            }

                            if let Err(e) = output.write_all(&bytes).await {
                                let _ = fs::remove_file(&temporary_path).await;
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({"ok": false, "where": "write_file", "error": e.to_string()})),
                                )
                                    .into_response();
                            }
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&temporary_path).await;
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({"ok": false, "where": "read_chunk", "error": e.to_string()})),
                            )
                                .into_response();
                        }
                    }
                }

                if let Err(e) = output.flush().await {
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"ok": false, "where": "flush", "error": e.to_string()})),
                    )
                        .into_response();
                }

                let Some(buffer) = first_sniff else {
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"ok": false, "where": "validation", "error": "empty file"})),
                    )
                        .into_response();
                };

                let Some(kind) = infer::get(&buffer) else {
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "ok": false,
                            "where": "validation",
                            "error": "unable to detect a supported video MIME type"
                        })),
                    )
                        .into_response();
                };

                if !kind.mime_type().starts_with("video/") {
                    let mime_type = kind.mime_type().to_string();
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "ok": false,
                            "where": "validation",
                            "error": format!("uploaded content is not a video: {}", mime_type)
                        })),
                    )
                        .into_response();
                }

                if let Err(e) = fs::rename(&temporary_path, &full_path).await {
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({"ok": false, "where": "rename", "error": e.to_string()})),
                    )
                        .into_response();
                }

                saved_filename_only = Some(filename);
                saved_path = Some(full_path);
            }
            _ => {}
        }
    }

    if file_count == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "where": "validation", "error": "missing file"})),
        )
            .into_response();
    }

    if title.is_empty() {
        title = "Untitled".to_string();
    }

    if title.chars().count() > MAX_TITLE_CHARS {
        if let Some(path) = saved_path.as_ref() {
            let _ = fs::remove_file(path).await;
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "where": "validation",
                "error": format!("title must not exceed {} characters", MAX_TITLE_CHARS)
            })),
        )
            .into_response();
    }

    if price_cents < 0 {
        if let Some(path) = saved_path.as_ref() {
            let _ = fs::remove_file(path).await;
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "where": "validation",
                "error": "price_cents must be zero or greater"
            })),
        )
            .into_response();
    }

    let (saved_path, saved_filename_only) = match (saved_path, saved_filename_only) {
        (Some(path), Some(filename)) => (path, filename),
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "ok": false,
                    "where": "upload_state",
                    "error": "uploaded file state is incomplete"
                })),
            )
                .into_response();
        }
    };

    let created_at = chrono::Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query!(
        r#"
        INSERT INTO videos
            (id, owner_id, title, price_cents, filename, created_at, hls_ready, processing_state)
        VALUES
            ($1, $2, $3, $4, $5, $6, FALSE, 'queued')
        "#,
        video_id,
        &user_id,
        title,
        price_cents,
        saved_filename_only,
        created_at
    )
    .execute(&st.pool)
    .await
    {
        let _ = fs::remove_file(&saved_path).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "where": "db_insert_videos", "error": e.to_string()})),
        )
            .into_response();
    }

    let output_dir = st.cfg.video_hls_dir(&video_id);
    if let Err(e) = st
        .worker
        .enqueue(TranscodeJob {
            video_id: video_id.clone(),
            input_path: saved_path.to_string_lossy().to_string(),
            out_dir: output_dir,
        })
        .await
    {
        let _ = sqlx::query!(
            r#"UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1"#,
            video_id,
            format!("enqueue: {e}")
        )
        .execute(&st.pool)
        .await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "where": "enqueue", "error": e.to_string()})),
        )
            .into_response();
    }

    info!(
        "upload ok: video_id={}, size={}",
        video_id,
        ByteSize(total_bytes)
    );

    (
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "video_id": video_id,
            "owner_id": user_id,
            "filename": saved_filename_only,
            "status": "queued",
            "message": "Upload succeeded. The video is being processed into HLS."
        })),
    )
        .into_response()
}
