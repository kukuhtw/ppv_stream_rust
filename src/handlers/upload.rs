// src/handlers/upload.rs
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tower_cookies::Cookies;
use serde_json::json;
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tokio::{fs, fs::File, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{config::Config, sessions};

#[derive(Clone)]
pub struct UploadState {
    pub cfg: Config,
    pub pool: PgPool,
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
                Json(json!({"ok":false,"where":"auth","error":"not logged in"})),
            ).into_response()
        }
    };

    // 2) Siapkan dir
    if let Err(e) = fs::create_dir_all(&st.cfg.storage_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"where":"storage_dir","error":e.to_string()})),
        ).into_response();
    }

    // 3) Ambil fields
    let mut title = "Untitled".to_string();
    let mut price: i64 = 0;
    let mut saved_path: Option<PathBuf> = None;
    let mut got_file = false;

    while let Some(field) = match mp.next_field().await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok":false,"where":"multipart","error":e.to_string()})),
            ).into_response()
        }
    } {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "title" => {
                title = field.text().await.unwrap_or_else(|_| "Untitled".into());
            }
            "price_cents" => {
                price = field
                    .text().await.unwrap_or_else(|_| "0".into())
                    .parse().unwrap_or(0);
            }
            "file" => {
                got_file = true;
                let fname = format!("{}.mp4", Uuid::new_v4());
                let path: PathBuf = Path::new(&st.cfg.storage_dir).join(&fname);

                let mut out = match File::create(&path).await {
                    Ok(f) => f,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({"ok":false,"where":"create_file","error":e.to_string()})),
                        ).into_response()
                    }
                };

                let mut f = field;
                while let Some(chunk) = f.chunk().await.transpose() {
                    match chunk {
                        Ok(bytes) => {
                            if let Err(e) = out.write_all(&bytes).await {
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({"ok":false,"where":"write_file","error":e.to_string()})),
                                ).into_response()
                            }
                        }
                        Err(e) => {
                            return (
                                StatusCode::BAD_REQUEST,
                                Json(json!({"ok":false,"where":"read_chunk","error":e.to_string()})),
                            ).into_response()
                        }
                    }
                }

                saved_path = Some(path);
            }
            _ => {}
        }
    }

    if !got_file {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok":false,"where":"validation","error":"missing file"})),
        ).into_response();
    }

    let path = saved_path.unwrap();

    // 4) Simpan metadata
    let vid = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    if let Err(e) = sqlx::query!(
        r#"
        INSERT INTO videos (id, owner_id, title, price_cents, filename, created_at)
        VALUES ($1,$2,$3,$4,$5,$6)
        "#,
        vid,
        &user_id,
        title,
        price,
        &path.to_string_lossy(),
        now
    )
    .execute(&st.pool)
    .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok":false,"where":"db_insert_videos","error":e.to_string()})),
        ).into_response();
    }

    (
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "video_id": vid,
            "owner_id": user_id,
            "filename": path.file_name().and_then(|s| s.to_str()).unwrap_or_default(),
            "message": "Upload sukses"
        })),
    )
        .into_response()
}
