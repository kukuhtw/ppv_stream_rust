// src/worker.rs
// use crate::{config::Config, ffmpeg, db};
use crate::{config::Config, ffmpeg};
use anyhow::Result;
use sqlx::PgPool;
use std::path::PathBuf;
use tokio::{
    sync::{mpsc, Semaphore},
    task::JoinHandle,
};

#[derive(Clone)]
pub struct TranscodeJob {
    pub video_id: String,
    pub input_path: String, // uploads/original/<id>.mp4
    pub out_dir: String,    // media/hls/<video_id>
}

#[derive(Clone)]
pub struct Worker {
    pub tx: mpsc::Sender<TranscodeJob>,
}

impl Worker {
    pub fn new(pool: PgPool, cfg: Config, concurrency: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<TranscodeJob>(1024);
        let sem = std::sync::Arc::new(Semaphore::new(concurrency.max(1)));
        // spawn loop
        let _handle: JoinHandle<()> = tokio::spawn({
            let sem = sem.clone();
            async move {
                while let Some(job) = rx.recv().await {
                    let permit = sem.clone().acquire_owned().await.unwrap();
                    let pool = pool.clone();
                    let cfg = cfg.clone();
                    tokio::spawn(async move {
                        let _p = permit;
                        let _ = process_job(&pool, &cfg, job).await;
                    });
                }
            }
        });
        Self { tx }
    }

    pub async fn enqueue(&self, job: TranscodeJob) -> Result<()> {
        self.tx.send(job).await.map_err(|e| anyhow::anyhow!(e))
    }
}

async fn process_job(pool: &PgPool, cfg: &Config, job: TranscodeJob) -> Result<()> {
    // update -> queued/processing
    sqlx::query!(
        "UPDATE videos SET processing_state = 'processing', last_error = NULL WHERE id = $1",
        job.video_id
    )
    .execute(pool)
    .await
    .ok();

    // 1) faststart ke tmp
    let tmp_mp4 = {
        let mut p = PathBuf::from(&cfg.tmp_dir);
        p.push(format!("{}.faststart.mp4", job.video_id));
        p.to_string_lossy().to_string()
    };
    if let Err(e) = ffmpeg::faststart_mp4(&job.input_path, &tmp_mp4).await {
        sqlx::query!(
            "UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1",
            job.video_id,
            e.to_string()
        )
        .execute(pool)
        .await
        .ok();
        return Err(e);
    }

    // 2) encode HLS ABR ke out_dir
    if let Err(e) = tokio::fs::create_dir_all(&job.out_dir).await {
        sqlx::query!(
            "UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1",
            job.video_id,
            e.to_string()
        )
        .execute(pool)
        .await
        .ok();
        return Err(anyhow::anyhow!(e));
    }

    // ⬇️ Tidak perlu unwrap_or: sudah u32
    match ffmpeg::encode_hls_abr(
        &tmp_mp4,
        &job.out_dir,
        &cfg.hwaccel,
        cfg.hls_segment_seconds,
    )
    .await
    {
        Ok(master_path) => {
            sqlx::query!(
                "UPDATE videos SET hls_ready = TRUE, hls_master = $2, processing_state='ready' WHERE id=$1",
                job.video_id,
                master_path
            )
            .execute(pool)
            .await
            .ok();

            // bersihkan tmp
            let _ = tokio::fs::remove_file(&tmp_mp4).await;
            Ok(())
        }
        Err(e) => {
            sqlx::query!(
                "UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1",
                job.video_id,
                e.to_string()
            )
            .execute(pool)
            .await
            .ok();
            // keep tmp for debugging
            Err(e)
        }
    }
}
