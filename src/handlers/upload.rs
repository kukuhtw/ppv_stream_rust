// src/handlers/upload.rs
// src/handlers/upload.rs
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use sqlx::PgPool;
use tower_cookies::Cookies;
use uuid::Uuid;

use std::path::{Path, PathBuf};
use tokio::{fs, fs::File, io::AsyncWriteExt};

use crate::{
    config::Config,
    sessions,
    worker::{TranscodeJob, Worker},
};

#[derive(Clone)]
pub struct UploadState {
    pub cfg: Config,
    pub pool: PgPool,
    pub worker: Worker, // ⬅️ penting: untuk enqueue job transcode
}

pub async fn upload_video(
    State(st): State<UploadState>,
    cookies: Cookies,
    mut mp: Multipart,
) -> impl IntoResponse {
    // 1) Auth
    let (user_id, _is_admin) = match sessions::current_user_id(&st.pool, &cookies).await {
        Some(v) => v,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"ok": false, "where":"auth", "error":"not logged in"})),
            )
                .into_response()
        }
    };

    // 2) Siapkan direktori upload (pakai upload_dir; fallback ke storage_dir bila kosong)
    let upload_dir = if !st.cfg.upload_dir.is_empty() {
        st.cfg.upload_dir.clone()
    } else {
        // fallback kompatibilitas lama
        st.cfg.storage_dir.clone()
    };
    if let Err(e) = fs::create_dir_all(&upload_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "where":"mkdir_upload", "error": e.to_string()})),
        )
            .into_response();
    }

    // 3) Ambil fields multipart
    let mut title = String::from("Untitled");
    let mut price_cents: i64 = 0;
    let mut got_file = false;

    // Untuk penamaan file dan record DB, gunakan 1 UUID yang sama
    let vid = Uuid::new_v4().to_string();

    // path file final (akan ditentukan setelah tahu ekstensi)
    let mut saved_path: Option<PathBuf> = None;
    let mut saved_filename_only: Option<String> = None;

    while let Some(field) = match mp.next_field().await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "where":"multipart", "error": e.to_string()})),
            )
                .into_response()
        }
    } {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "title" => {
                title = field.text().await.unwrap_or_else(|_| "Untitled".into());
            }
            "price_cents" => {
                price_cents = field
                    .text()
                    .await
                    .unwrap_or_else(|_| "0".into())
                    .parse()
                    .unwrap_or(0);
            }
            "file" => {
                got_file = true;

                // Tentukan ekstensi dari filename; default "mp4"
                let mut f = field;
                let ext = f
                    .file_name()
                    .and_then(|s| Path::new(s).extension().and_then(|e| e.to_str()))
                    .map(|s| s.to_lowercase())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "mp4".to_string());

                let fname = format!("{}.{}", &vid, ext);
                let full_path: PathBuf = Path::new(&upload_dir).join(&fname);

                let mut out =
                    match File::create(&full_path).await {
                        Ok(fh) => fh,
                        Err(e) => return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(
                                json!({"ok": false, "where":"create_file", "error": e.to_string()}),
                            ),
                        )
                            .into_response(),
                    };

                // Tulis per-chunk agar hemat memori
                while let Some(chunk_res) = f.chunk().await.transpose() {
                    match chunk_res {
                        Ok(bytes) => {
                            if let Err(e) = out.write_all(&bytes).await {
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({"ok": false, "where":"write_file", "error": e.to_string()})),
                                )
                                    .into_response();
                            }
                        }
                        Err(e) => return (
                            StatusCode::BAD_REQUEST,
                            Json(
                                json!({"ok": false, "where":"read_chunk", "error": e.to_string()}),
                            ),
                        )
                            .into_response(),
                    }
                }

                saved_filename_only = Some(fname);
                saved_path = Some(full_path);
            }
            _ => {
                // abaikan field lain
            }
        }
    }

    // Validasi ada file
    if !got_file {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "where":"validation", "error":"missing file"})),
        )
            .into_response();
    }
    let (saved_path, saved_filename_only) = (saved_path.unwrap(), saved_filename_only.unwrap());

    // 4) Insert metadata video ke DB (status: queued)
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query!(
        r#"
        INSERT INTO videos (id, owner_id, title, price_cents, filename, created_at, hls_ready, processing_state)
        VALUES ($1,       $2,       $3,    $4,          $5,      $6,        FALSE,       'queued')
        "#,
        vid,
        &user_id,
        title,
        price_cents,
        saved_filename_only, // simpan nama file saja (lebih portabel)
        now
    )
    .execute(&st.pool)
    .await
    {
        // Bila insert DB gagal, hapus file yang baru disimpan agar tidak jadi orphan
        let _ = fs::remove_file(&saved_path).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "where":"db_insert_videos", "error": e.to_string()})),
        )
            .into_response();
    }

    // 5) Enqueue job transcode → hasilkan HLS di media_dir/<video_id>/
    let out_dir = st.cfg.video_hls_dir(&vid);
    if let Err(e) = st
        .worker
        .enqueue(TranscodeJob {
            video_id: vid.clone(),
            input_path: saved_path.to_string_lossy().to_string(),
            out_dir,
        })
        .await
    {
        // Jika enqueue gagal, tandai error di DB (supaya UI bisa tampilkan status)
        let _ = sqlx::query!(
            r#"UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1"#,
            vid,
            format!("enqueue: {e}")
        )
        .execute(&st.pool)
        .await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "where":"enqueue", "error": e.to_string()})),
        )
            .into_response();
    }

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
    )
        .into_response()
}
