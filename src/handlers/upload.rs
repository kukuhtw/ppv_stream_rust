// src/handlers/upload.rs
use axum::{ extract::{Multipart, State}, http::StatusCode, response::IntoResponse, Json };
use serde_json::json;
use sqlx::PgPool;
use tower_cookies::Cookies;
use uuid::Uuid;

use std::path::{Path, PathBuf};
use tokio::{fs, fs::File, io::{AsyncWriteExt, BufWriter}};
use infer;
use tracing::{info, warn};
use bytesize::ByteSize;

use crate::{ config::Config, sessions, worker::{TranscodeJob, Worker} };

#[derive(Clone)]
pub struct UploadState {
    pub cfg: Config,
    pub pool: PgPool,
    pub worker: Worker,
}

pub async fn upload_video(
    State(st): State<UploadState>,
    cookies: Cookies,
    mut mp: Multipart,
) -> impl IntoResponse {
    // 1) Auth
   
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "where":"auth", "error":"not logged in"})),
            ).into_response()
        }
    };

    // 2) Siapkan dir upload
    let upload_dir = if !st.cfg.upload_dir.is_empty() { st.cfg.upload_dir.clone() } else { st.cfg.storage_dir.clone() };
    if let Err(e) = fs::create_dir_all(&upload_dir).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"mkdir_upload", "error": e.to_string()}))).into_response();
    }

    // 3) Baca multipart
    let mut title = String::from("Untitled");
    let mut price_cents: i64 = 0;
    let mut got_file = false;

    // satu UUID untuk id video & nama file
    let vid = Uuid::new_v4().to_string();

    let mut saved_path: Option<PathBuf> = None;
    let mut saved_filename_only: Option<String> = None;

    // Track ukuran untuk batas
    let mut total_bytes: u64 = 0;
    let max_bytes = st.cfg.max_upload_bytes;

    // whitelist ekstensi
    let allows: Vec<String> = st.cfg.allow_exts.iter().map(|s| s.to_ascii_lowercase()).collect();

    while let Some(field) = match mp.next_field().await {
        Ok(f) => f,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "where":"multipart", "error": e.to_string()}))).into_response()
        }
    } {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "title" => {
                title = field.text().await.unwrap_or_else(|_| "Untitled".into());
            }
            "price_cents" => {
                price_cents = field.text().await.unwrap_or_else(|_| "0".into()).parse().unwrap_or(0);
            }
            "file" => {
                got_file = true;
                let mut f = field;

                // Tentukan ekstensi (default mp4); validasi whitelist
                let ext = f
                    .file_name()
                    .and_then(|s| Path::new(s).extension().and_then(|e| e.to_str()))
                    .map(|s| s.to_lowercase())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "mp4".to_string());

                if !allows.contains(&ext) {
                    return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "where":"validation", "error": format!("file extension not allowed: .{}", ext)}))).into_response();
                }

                // Tulis ke temporary .part lalu rename
                let fname = format!("{}.{}", &vid, ext);
                let full_path: PathBuf = Path::new(&upload_dir).join(&fname);
                let tmp_path = full_path.with_extension(format!("{}.part", ext));
                let tmp_dir = tmp_path.parent().unwrap().to_path_buf();
                fs::create_dir_all(&tmp_dir).await.ok();

                let of = match File::create(&tmp_path).await {
                    Ok(fh) => fh,
                    Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"create_file", "error": e.to_string()}))).into_response(),
                };
                // buffer besar untuk I/O hemat syscall
                let mut out = BufWriter::with_capacity(1 * 1024 * 1024, of);

                // Sniff awal untuk cek MIME (opsional ketat)
                let mut first_sniff: Option<Vec<u8>> = None;

                while let Some(chunk_res) = f.chunk().await.transpose() {
                    match chunk_res {
                        Ok(bytes) => {
                            total_bytes += bytes.len() as u64;
                            if total_bytes > max_bytes {
                                let _ = fs::remove_file(&tmp_path).await;
                                return (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"ok": false, "where":"validation", "error": format!("file too large: {} > {}", ByteSize(total_bytes), ByteSize(max_bytes))}))).into_response();
                            }
                            if first_sniff.is_none() {
                                first_sniff = Some(bytes[..bytes.len().min(8192)].to_vec());
                            }
                            if let Err(e) = out.write_all(&bytes).await {
                                let _ = fs::remove_file(&tmp_path).await;
                                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"write_file", "error": e.to_string()}))).into_response();
                            }
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&tmp_path).await;
                            return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "where":"read_chunk", "error": e.to_string()}))).into_response();
                        }
                    }
                }
                if let Err(e) = out.flush().await {
                    let _ = fs::remove_file(&tmp_path).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"flush", "error": e.to_string()}))).into_response();
                }

                // Validasi MIME sederhana via infer (best-effort)
                if let Some(buf) = first_sniff {
                    if let Some(kind) = infer::get(&buf) {
                        let mime = kind.mime_type();
                        // beberapa video mime biasa: video/mp4, video/x-matroska, video/quicktime, video/webm
                        let ok = mime.starts_with("video/");
                        if !ok {
                            warn!("upload mime suspect: {}", mime);
                        }
                    }
                }

                // Atomic rename
                if let Err(e) = fs::rename(&tmp_path, &full_path).await {
                    let _ = fs::remove_file(&tmp_path).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"rename", "error": e.to_string()}))).into_response();
                }

                saved_filename_only = Some(fname);
                saved_path = Some(full_path);
            }
            _ => { /* ignore */ }
        }
    }

    if !got_file {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "where":"validation", "error":"missing file"}))).into_response();
    }
    let (saved_path, saved_filename_only) = (saved_path.unwrap(), saved_filename_only.unwrap());

    // 4) Insert metadata video
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query!(
        r#"
        INSERT INTO videos (id, owner_id, title, price_cents, filename, created_at, hls_ready, processing_state)
        VALUES ($1, $2, $3, $4, $5, $6, FALSE, 'queued')
        "#,
        vid, &user_id, title, price_cents, saved_filename_only, now
    ).execute(&st.pool).await {
        let _ = fs::remove_file(&saved_path).await;
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"db_insert_videos", "error": e.to_string()}))).into_response();
    }

    // 5) Enqueue transcode (CPU-only)
    let out_dir = st.cfg.video_hls_dir(&vid);
    if let Err(e) = st.worker.enqueue(TranscodeJob {
        video_id: vid.clone(),
        input_path: saved_path.to_string_lossy().to_string(),
        out_dir,
    }).await {
        let _ = sqlx::query!(
            r#"UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1"#,
            vid, format!("enqueue: {e}")
        ).execute(&st.pool).await;

        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "where":"enqueue", "error": e.to_string()}))).into_response();
    }

    info!("upload ok: video_id={}, size={}", vid, ByteSize(total_bytes));

    // 6) Response
    (
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "video_id": vid,
            "owner_id": user_id,
            "filename": saved_filename_only,
            "status": "queued",
            "message": "Upload sukses. Video sedang diproses menjadi HLS."
        })),
    ).into_response()
}

