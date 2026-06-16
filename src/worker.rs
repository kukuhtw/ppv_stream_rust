// src/worker.rs
// Background transcoding queue and FFmpeg job processor.

use crate::{config::Config, ffmpeg::run_ffmpeg, plugins::storage::StoragePlugin};
use anyhow::{anyhow, Context, Result};
use sqlx::PgPool;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    fs,
    sync::{mpsc, Semaphore},
    task::JoinHandle,
};
use tracing::{error, info, warn};

#[derive(Clone, Debug)]
pub struct TranscodeJob {
    pub video_id: String,
    pub input_path: String,
    pub out_dir: String,
}

#[derive(Clone)]
pub struct Worker {
    pub tx: mpsc::Sender<TranscodeJob>,
}

impl Worker {
    pub fn new(
        pool: PgPool,
        cfg: Config,
        storage: Arc<dyn StoragePlugin>,
        concurrency: usize,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<TranscodeJob>(1024);
        let semaphore = Arc::new(Semaphore::new(concurrency.max(1)));

        let _handle: JoinHandle<()> = tokio::spawn({
            let semaphore = semaphore.clone();
            let storage = storage.clone();
            async move {
                while let Some(job) = rx.recv().await {
                    let permit = match semaphore.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(e) => {
                            error!("worker semaphore closed: {e}");
                            break;
                        }
                    };
                    let pool = pool.clone();
                    let cfg = cfg.clone();
                    let storage = storage.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        if let Err(e) = process_job(&pool, &cfg, storage, job).await {
                            error!("transcode job failed: {e}");
                        }
                    });
                }
            }
        });

        Self { tx }
    }

    pub async fn enqueue(&self, job: TranscodeJob) -> Result<()> {
        self.tx.send(job).await.map_err(|e| anyhow!(e))
    }
}

async fn update_video_error(pool: &PgPool, video_id: &str, message: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1",
        video_id,
        message
    )
    .execute(pool)
    .await
    .with_context(|| format!("update error state for video {video_id}"))?;
    Ok(())
}

async fn process_job(
    pool: &PgPool,
    cfg: &Config,
    storage: Arc<dyn StoragePlugin>,
    job: TranscodeJob,
) -> Result<()> {
    sqlx::query!(
        "UPDATE videos SET processing_state = 'processing', last_error = NULL WHERE id = $1",
        job.video_id
    )
    .execute(pool)
    .await
    .with_context(|| format!("mark video {} as processing", job.video_id))?;

    let tmp_mp4 = {
        let mut path = PathBuf::from(&cfg.tmp_dir);
        fs::create_dir_all(&path)
            .await
            .with_context(|| format!("create temp directory {}", path.display()))?;
        path.push(format!("{}.faststart.mp4", job.video_id));
        path.to_string_lossy().to_string()
    };

    if let Err(e) = faststart_mp4(&job.input_path, &tmp_mp4).await {
        if let Err(update_err) = update_video_error(pool, &job.video_id, &e.to_string()).await {
            error!("failed to persist transcoding error: {update_err}");
        }
        let _ = fs::remove_file(&tmp_mp4).await;
        return Err(e);
    }

    if let Err(e) = fs::create_dir_all(&job.out_dir).await {
        if let Err(update_err) = update_video_error(pool, &job.video_id, &e.to_string()).await {
            error!("failed to persist output directory error: {update_err}");
        }
        let _ = fs::remove_file(&tmp_mp4).await;
        return Err(anyhow!(e));
    }

    let encode_result = encode_hls_abr(
        &tmp_mp4,
        &job.out_dir,
        &cfg.hwaccel,
        cfg.hls_segment_seconds,
    )
    .await;

    match encode_result {
        Ok(master_name) => {
            let master_abs = Path::new(&job.out_dir).join(&master_name);
            let master_abs_owned = master_abs.to_string_lossy().into_owned();

            if let Err(e) = sqlx::query!(
                "UPDATE videos SET hls_ready = TRUE, hls_master = $2, processing_state='ready', last_error=NULL WHERE id=$1",
                job.video_id,
                master_abs_owned.as_str()
            )
            .execute(pool)
            .await
            {
                let _ = fs::remove_file(&tmp_mp4).await;
                return Err(anyhow!(e).context(format!(
                    "mark video {} as ready",
                    job.video_id
                )));
            }

            let _ = fs::remove_file(&tmp_mp4).await;
            info!(
                "transcode done: video_id={}, master={}",
                job.video_id,
                master_abs.display()
            );

            // Push HLS output to remote storage backend (fire-and-forget, non-fatal).
            // No-op when STORAGE_BACKEND=local.
            if !storage.is_local() {
                let storage_clone = storage.clone();
                let prefix = format!("videos/{}", job.video_id);
                let out_dir_clone = job.out_dir.clone();
                let video_id_clone = job.video_id.clone();
                tokio::spawn(async move {
                    match storage_clone
                        .put_dir(&prefix, Path::new(&out_dir_clone))
                        .await
                    {
                        Ok(n) => info!(
                            "storage: synced {n} HLS files for {video_id_clone} to {}",
                            storage_clone.backend_name()
                        ),
                        Err(e) => warn!("storage: HLS sync for {video_id_clone} non-fatal: {e}"),
                    }
                });
            }

            Ok(())
        }
        Err(e) => {
            if let Err(update_err) = update_video_error(pool, &job.video_id, &e.to_string()).await {
                error!("failed to persist transcoding error: {update_err}");
            }

            if let Err(remove_err) = fs::remove_file(&tmp_mp4).await {
                if remove_err.kind() != std::io::ErrorKind::NotFound {
                    warn!("failed to remove temp file {}: {}", tmp_mp4, remove_err);
                }
            }

            if let Err(remove_err) = fs::remove_dir_all(&job.out_dir).await {
                if remove_err.kind() != std::io::ErrorKind::NotFound {
                    warn!(
                        "failed to remove incomplete HLS directory {}: {}",
                        job.out_dir, remove_err
                    );
                }
            }

            Err(e)
        }
    }
}

async fn faststart_mp4(input: &str, output: &str) -> Result<()> {
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-map".into(),
        "0".into(),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.into(),
    ];
    let work_dir = Path::new(output)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy()
        .to_string();
    run_ffmpeg(&args, &work_dir).await
}

async fn encode_hls_abr(
    input: &str,
    out_dir: &str,
    _hwaccel: &str,
    seg_secs: u32,
) -> Result<String> {
    let filter_complex = "\
[0:v]split=3[v0][v1][v2];\
[v0]scale=w=426:h=240:force_original_aspect_ratio=decrease:eval=frame[v0o];\
[v1]scale=w=640:h=360:force_original_aspect_ratio=decrease:eval=frame[v1o];\
[v2]scale=w=854:h=480:force_original_aspect_ratio=decrease:eval=frame[v2o]";

    let master_name = "master.m3u8".to_string();

    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-filter_complex".into(),
        filter_complex.into(),
        "-map".into(),
        "[v0o]".into(),
        "-map".into(),
        "a:0?".into(),
        "-map".into(),
        "[v1o]".into(),
        "-map".into(),
        "a:0?".into(),
        "-map".into(),
        "[v2o]".into(),
        "-map".into(),
        "a:0?".into(),
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
        "-b:v:0".into(),
        "400k".into(),
        "-maxrate:v:0".into(),
        "440k".into(),
        "-bufsize:v:0".into(),
        "800k".into(),
        "-b:a:0".into(),
        "96k".into(),
        "-b:v:1".into(),
        "800k".into(),
        "-maxrate:v:1".into(),
        "880k".into(),
        "-bufsize:v:1".into(),
        "1600k".into(),
        "-b:a:1".into(),
        "128k".into(),
        "-b:v:2".into(),
        "1400k".into(),
        "-maxrate:v:2".into(),
        "1540k".into(),
        "-bufsize:v:2".into(),
        "2800k".into(),
        "-b:a:2".into(),
        "128k".into(),
        "-threads".into(),
        format!("{}", num_cpus::get().max(2)),
        "-f".into(),
        "hls".into(),
        "-hls_time".into(),
        seg_secs.to_string(),
        "-hls_playlist_type".into(),
        "vod".into(),
        "-hls_flags".into(),
        "independent_segments".into(),
        "-hls_segment_filename".into(),
        "v%v/seg_%05d.ts".into(),
        "-master_pl_name".into(),
        master_name.clone(),
        "-var_stream_map".into(),
        "v:0,a:0 v:1,a:1 v:2,a:2".into(),
        "v%v/index.m3u8".into(),
    ];

    run_work_dir(out_dir, || run_ffmpeg(&args, out_dir)).await?;
    Ok(master_name)
}

async fn run_work_dir<F, Fut>(dir: &str, function: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    for subdirectory in ["v0", "v1", "v2"] {
        let path = Path::new(dir).join(subdirectory);
        fs::create_dir_all(&path)
            .await
            .with_context(|| format!("create HLS subdirectory {}", path.display()))?;
    }
    function().await
}
