// src/worker.rs
// src/worker.rs
// queue sederhana

use crate::{config::Config, ffmpeg::run_ffmpeg};
use anyhow::{anyhow, Result};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    sync::{mpsc, Semaphore},
    task::JoinHandle,
};
use tracing::{error, info};

#[derive(Clone, Debug)]
pub struct TranscodeJob {
    pub video_id: String,
    pub input_path: String, // uploads/original/<id>.mp4
    pub out_dir: String,    // media/<video_id>
}

#[derive(Clone)]
pub struct Worker {
    pub tx: mpsc::Sender<TranscodeJob>,
}

impl Worker {
    pub fn new(pool: PgPool, cfg: Config, concurrency: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<TranscodeJob>(1024);
        let sem = std::sync::Arc::new(Semaphore::new(concurrency.max(1)));

        let _handle: JoinHandle<()> = tokio::spawn({
            let sem = sem.clone();
            async move {
                while let Some(job) = rx.recv().await {
                    let permit = sem.clone().acquire_owned().await.unwrap();
                    let pool = pool.clone();
                    let cfg = cfg.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        if let Err(e) = process_job(&pool, &cfg, job).await {
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

async fn process_job(pool: &PgPool, cfg: &Config, job: TranscodeJob) -> Result<()> {
    // Tandai processing
    sqlx::query!(
        "UPDATE videos SET processing_state = 'processing', last_error = NULL WHERE id = $1",
        job.video_id
    )
    .execute(pool)
    .await
    .ok();

    // Siapkan tmp mp4 untuk faststart
    let tmp_mp4 = {
        let mut p = PathBuf::from(&cfg.tmp_dir);
        fs::create_dir_all(&p).await.ok();
        p.push(format!("{}.faststart.mp4", job.video_id));
        p.to_string_lossy().to_string()
    };

    // 1) MP4 faststart (copy stream, atom di depan)
    if let Err(e) = faststart_mp4(&job.input_path, &tmp_mp4).await {
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

    // 2) Pastikan out_dir ada
    if let Err(e) = fs::create_dir_all(&job.out_dir).await {
        sqlx::query!(
            "UPDATE videos SET processing_state='error', last_error=$2 WHERE id=$1",
            job.video_id,
            e.to_string()
        )
        .execute(pool)
        .await
        .ok();
        return Err(anyhow!(e));
    }

    // 3) Encode HLS ABR (multi-rendition) → master.m3u8
    match encode_hls_abr(&tmp_mp4, &job.out_dir, &cfg.hwaccel, cfg.hls_segment_seconds).await {
        Ok(master_name) => {
            let master_abs = Path::new(&job.out_dir).join(&master_name);
            // === FIX E0716: simpan ke owned String agar hidup melewati .await ===
            let master_abs_owned = master_abs.to_string_lossy().into_owned();

            sqlx::query!(
                "UPDATE videos SET hls_ready = TRUE, hls_master = $2, processing_state='ready' WHERE id=$1",
                job.video_id,
                master_abs_owned.as_str()
            )
            .execute(pool)
            .await
            .ok();

            // hapus tmp
            let _ = fs::remove_file(&tmp_mp4).await;
            info!(
                "transcode done: video_id={}, master={}",
                job.video_id,
                master_abs.display()
            );
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
            // biarkan tmp untuk debugging
            Err(e)
        }
    }
}

/// Jalankan ffmpeg untuk membuat MP4 dengan `+faststart` (copy stream)
async fn faststart_mp4(input: &str, output: &str) -> Result<()> {
    // -movflags +faststart akan memindahkan moov atom ke awal file
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
    // kerja di direktori output agar file relatif
    let work_dir = Path::new(output)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy()
        .to_string();
    run_ffmpeg(&args, &work_dir).await
}

/// Encode HLS ABR 3-rendition (240p/360p/480p) dengan satu proses ffmpeg.
/// Mengembalikan nama file master playlist (mis. "master.m3u8") relatif terhadap `out_dir`.
async fn encode_hls_abr(
    input: &str,
    out_dir: &str,
    _hwaccel: &str,
    seg_secs: u32,
) -> Result<String> {
    // Catatan: GPU tidak digunakan (server tanpa GPU). Jika nanti mau NVENC/VAAPI,
    // tambahkan mapping codec sesuai _hwaccel.

    // Skala 3 varian: 240p / 360p / 480p
    // Audio 2ch AAC 96k untuk 240p, 128k untuk lainnya (cukup umum).
    // Bitrate video konservatif supaya CPU encode kuat di server kecil.
    let filter_complex = "\
[0:v]split=3[v0][v1][v2];\
[v0]scale=w=426:h=240:force_original_aspect_ratio=decrease:eval=frame[v0o];\
[v1]scale=w=640:h=360:force_original_aspect_ratio=decrease:eval=frame[v1o];\
[v2]scale=w=854:h=480:force_original_aspect_ratio=decrease:eval=frame[v2o]";

    // Direktori kerja: out_dir
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
        // mapping 3 varian video + audio (opsional a:0)
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
        // codec & preset (CPU)
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
        // bitrate per varian (0,1,2)
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
        // HLS settings
        "-f".into(),
        "hls".into(),
        "-hls_time".into(),
        seg_secs.to_string(),
        "-hls_playlist_type".into(),
        "vod".into(),
        "-hls_flags".into(),
        "independent_segments".into(),
        // subdir per varian: v%v
        "-hls_segment_filename".into(),
        "v%v/seg_%05d.ts".into(),
        "-master_pl_name".into(),
        master_name.clone(),
        "-var_stream_map".into(),
        "v:0,a:0 v:1,a:1 v:2,a:2".into(),
        // Output playlists
        "v%v/index.m3u8".into(),
    ];

    run_work_dir(out_dir, || run_ffmpeg(&args, out_dir)).await?;
    Ok(master_name)
}

async fn run_work_dir<F, Fut>(dir: &str, f: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    // Pastikan subdir v0,v1,v2 ada agar -hls_segment_filename berhasil
    for sub in ["v0", "v1", "v2"] {
        let p = Path::new(dir).join(sub);
        if fs::create_dir_all(&p).await.is_err() {
            // lanjut saja, ffmpeg bisa buat kalau izin cukup
        }
    }
    f().await
}
