// src/handlers/upload.rs
//
// Video upload endpoint and upload-to-transcoding handoff.
//
// This module is responsible for:
// 1. Authenticating the uploader.
// 2. Reading multipart form fields without loading the whole video into memory.
// 3. Validating file extension, size, and best-effort MIME type.
// 4. Writing the upload safely through a temporary `.part` file.
// 5. Inserting the video metadata into PostgreSQL.
// 6. Enqueuing a background transcoding job.

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
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    config::Config,
    sessions,
    worker::{TranscodeJob, Worker},
};

/// Shared Axum state for the upload handler.
///
/// `cfg` provides upload limits and storage paths.
/// `pool` is used to persist video metadata and processing state.
/// `worker` receives the asynchronous transcoding job after the upload succeeds.
#[derive(Clone)]
pub struct UploadState {
    pub cfg: Config,
    pub pool: PgPool,
    pub worker: Worker,
}

/// Accepts a multipart video upload and queues it for HLS transcoding.
///
/// Expected multipart fields:
/// * `title`
/// * `price_cents`
/// * `file`
///
/// Processing flow:
/// 1. Authenticate the current user.
/// 2. Resolve and prepare the upload directory.
/// 3. Parse metadata and stream the file to disk.
/// 4. Validate extension, size, and detected MIME type.
/// 5. Atomically finalize the uploaded file.
/// 6. Insert a queued video record.
/// 7. Enqueue the background transcoding job.
/// 8. Return the new video ID and queued state.
pub async fn upload_video(
    State(st): State<UploadState>,
    cookies: Cookies,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // ---------------------------------------------------------------------
    // Step 1: Authenticate the uploader
    // ---------------------------------------------------------------------

    // Validate the signed session cookie and load the user ID from the
    // server-side sessions table. Anonymous users cannot upload videos.
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "ok": false,
                    "where": "auth",
                    "error": "not logged in"
                })),
            )
                .into_response()
        }
    };

    // ---------------------------------------------------------------------
    // Step 2: Resolve and prepare the upload directory
    // ---------------------------------------------------------------------

    // Prefer the current `upload_dir` setting. Fall back to the legacy
    // `storage_dir` alias when `upload_dir` is empty.
    let upload_dir = if !st.cfg.upload_dir.is_empty() {
        st.cfg.upload_dir.clone()
    } else {
        st.cfg.storage_dir.clone()
    };

    // Ensure the directory exists before opening any upload file.
    if let Err(e) = fs::create_dir_all(&upload_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "ok": false,
                "where": "mkdir_upload",
                "error": e.to_string()
            })),
        )
            .into_response();
    }

    // ---------------------------------------------------------------------
    // Step 3: Initialize multipart parsing state
    // ---------------------------------------------------------------------

    // Use safe defaults when optional metadata fields are missing or invalid.
    let mut title = String::from("Untitled");
    let mut price_cents: i64 = 0;
    let mut got_file = false;

    // One UUID is used for both the database video ID and the generated filename.
    // This avoids trusting or storing the original client-provided filename.
    let video_id = Uuid::new_v4().to_string();

    // These values are populated after the uploaded file is fully written and
    // atomically renamed from its temporary path.
    let mut saved_path: Option<PathBuf> = None;
    let mut saved_filename_only: Option<String> = None;

    // Track the received byte count while streaming so oversized uploads can be
    // rejected before the entire request is written to disk.
    let mut total_bytes: u64 = 0;
    let max_bytes = st.cfg.max_upload_bytes;

    // Normalize the configured extension whitelist for case-insensitive checks.
    let allowed_extensions: Vec<String> = st
        .cfg
        .allow_exts
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();

    // ---------------------------------------------------------------------
    // Step 4: Read multipart fields one by one
    // ---------------------------------------------------------------------

    while let Some(field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "ok": false,
                    "where": "multipart",
                    "error": e.to_string()
                })),
            )
                .into_response()
        }
    } {
        let field_name = field.name().unwrap_or_default().to_string();

        match field_name.as_str() {
            // Read the human-readable title as a small text field.
            "title" => {
                title = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "Untitled".into());
            }

            // Parse the price into the application's smallest configured unit.
            // Invalid values fall back to zero rather than failing the whole upload.
            "price_cents" => {
                price_cents = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "0".into())
                    .parse()
                    .unwrap_or(0);
            }

            // Stream the uploaded video file to disk in chunks.
            "file" => {
                got_file = true;
                let mut file_field = field;

                // Determine the extension from the original filename. When the
                // extension is absent, default to `mp4` for compatibility.
                let extension = file_field
                    .file_name()
                    .and_then(|name| Path::new(name).extension().and_then(|ext| ext.to_str()))
                    .map(|ext| ext.to_ascii_lowercase())
                    .filter(|ext| !ext.is_empty())
                    .unwrap_or_else(|| "mp4".to_string());

                // Reject extensions that are not explicitly enabled in Config.
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

                // Build a server-controlled filename from the generated UUID.
                let filename = format!("{}.{}", &video_id, extension);
                let full_path: PathBuf = Path::new(&upload_dir).join(&filename);

                // Write to a temporary `.part` file first. The final filename is
                // only exposed after the complete upload has been written.
                let temporary_path = full_path.with_extension(format!("{}.part", extension));
                let temporary_dir = temporary_path
                    .parent()
                    .unwrap_or_else(|| Path::new(&upload_dir))
                    .to_path_buf();

                // Best-effort directory preparation. The following file creation
                // still returns a clear error if directory creation did not succeed.
                let _ = fs::create_dir_all(&temporary_dir).await;

                let output_file = match File::create(&temporary_path).await {
                    Ok(file_handle) => file_handle,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({
                                "ok": false,
                                "where": "create_file",
                                "error": e.to_string()
                            })),
                        )
                            .into_response()
                    }
                };

                // Use a one MiB buffer to reduce the number of filesystem write
                // operations during large video uploads.
                let mut output = BufWriter::with_capacity(1024 * 1024, output_file);

                // Keep only the first few KiB for best-effort MIME detection.
                // The complete upload is never copied into memory.
                let mut first_sniff: Option<Vec<u8>> = None;

                // Read and persist every multipart chunk asynchronously.
                while let Some(chunk_result) = file_field.chunk().await.transpose() {
                    match chunk_result {
                        Ok(bytes) => {
                            total_bytes += bytes.len() as u64;

                            // Remove the temporary file immediately when the upload
                            // exceeds the configured limit.
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

                            // Capture at most the first 8192 bytes for MIME sniffing.
                            if first_sniff.is_none() {
                                first_sniff = Some(bytes[..bytes.len().min(8192)].to_vec());
                            }

                            // Write the current chunk to the buffered temporary file.
                            if let Err(e) = output.write_all(&bytes).await {
                                let _ = fs::remove_file(&temporary_path).await;
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({
                                        "ok": false,
                                        "where": "write_file",
                                        "error": e.to_string()
                                    })),
                                )
                                    .into_response();
                            }
                        }
                        Err(e) => {
                            // Multipart read failures leave an incomplete file, so
                            // remove it before returning a client error.
                            let _ = fs::remove_file(&temporary_path).await;
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({
                                    "ok": false,
                                    "where": "read_chunk",
                                    "error": e.to_string()
                                })),
                            )
                                .into_response();
                        }
                    }
                }

                // Flush buffered bytes so the complete upload reaches the filesystem
                // before MIME inspection and final rename.
                if let Err(e) = output.flush().await {
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "ok": false,
                            "where": "flush",
                            "error": e.to_string()
                        })),
                    )
                        .into_response();
                }

                // Perform best-effort MIME detection from the initial bytes.
                // A suspicious type is logged but not rejected because container
                // formats and uncommon video codecs may be detected inconsistently.
                if let Some(buffer) = first_sniff {
                    if let Some(kind) = infer::get(&buffer) {
                        let mime_type = kind.mime_type();
                        if !mime_type.starts_with("video/") {
                            warn!("upload mime suspect: {}", mime_type);
                        }
                    }
                }

                // Atomically rename the complete `.part` file to its final name.
                // Consumers should never observe a partially written final file.
                if let Err(e) = fs::rename(&temporary_path, &full_path).await {
                    let _ = fs::remove_file(&temporary_path).await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "ok": false,
                            "where": "rename",
                            "error": e.to_string()
                        })),
                    )
                        .into_response();
                }

                saved_filename_only = Some(filename);
                saved_path = Some(full_path);
            }

            // Ignore unknown multipart fields so clients may add optional metadata
            // without breaking older server versions.
            _ => {}
        }
    }

    // A successful upload request must contain exactly one processed file field.
    if !got_file {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "where": "validation",
                "error": "missing file"
            })),
        )
            .into_response();
    }

    // These values are guaranteed to exist after a successful file field. The
    // explicit match avoids silently continuing with incomplete upload state.
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

    // ---------------------------------------------------------------------
    // Step 5: Persist video metadata
    // ---------------------------------------------------------------------

    // Store the video as queued. The worker will later transition the processing
    // state to processing, ready, or error.
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
        // Remove the saved file when metadata persistence fails so the upload
        // directory does not accumulate orphaned media.
        let _ = fs::remove_file(&saved_path).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "ok": false,
                "where": "db_insert_videos",
                "error": e.to_string()
            })),
        )
            .into_response();
    }

    // ---------------------------------------------------------------------
    // Step 6: Enqueue background HLS transcoding
    // ---------------------------------------------------------------------

    // Derive the final HLS directory from the configured media root and video ID.
    let output_dir = st.cfg.video_hls_dir(&video_id);

    // Move the source path and destination directory into a TranscodeJob. The
    // worker processes it asynchronously, so the HTTP request does not perform
    // the expensive FFmpeg conversion itself.
    if let Err(e) = st
        .worker
        .enqueue(TranscodeJob {
            video_id: video_id.clone(),
            input_path: saved_path.to_string_lossy().to_string(),
            out_dir: output_dir,
        })
        .await
    {
        // Preserve the database record for operational visibility, but mark the
        // processing state as failed because the job never reached the worker.
        let _ = sqlx::query!(
            r#"UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1"#,
            video_id,
            format!("enqueue: {e}")
        )
        .execute(&st.pool)
        .await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "ok": false,
                "where": "enqueue",
                "error": e.to_string()
            })),
        )
            .into_response();
    }

    info!(
        "upload ok: video_id={}, size={}",
        video_id,
        ByteSize(total_bytes)
    );

    // ---------------------------------------------------------------------
    // Step 7: Return the queued upload response
    // ---------------------------------------------------------------------

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
