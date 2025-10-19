
// src/ffmpeg.rs
use anyhow::{anyhow, Result};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Jalankan ffmpeg async dengan argumen lengkap dari pemanggil.
/// Dipakai oleh pipeline HLS (transcode/segment/burn-in watermark).
pub async fn transcode_hls(_input_path: &str, _session_dir: &str, args: Vec<String>) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn ffmpeg: {e}"))?;

    let mut stderr = child.stderr.take().unwrap();
    let mut err_buf = Vec::new();
    let stderr_task = tokio::spawn(async move {
        let mut tmp = [0u8; 8192];
        while let Ok(n) = stderr.read(&mut tmp).await {
            if n == 0 {
                break;
            }
            err_buf.extend_from_slice(&tmp[..n]);
        }
        err_buf
    });

    let status = child.wait().await.map_err(|e| anyhow!("wait ffmpeg: {e}"))?;
    let err_bytes = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let err_str = String::from_utf8_lossy(&err_bytes);
        return Err(anyhow!(
            "ffmpeg exit code {:?}. stderr:\n{}",
            status.code(),
            err_str
        ));
    }

    Ok(())
}

/// Remux MP4 agar `moov` dipindah ke awal file (progressive playback).
/// - Lossless & cepat (pakai `-c copy`)
/// - Aman dipakai setelah upload sebelum file disegment untuk HLS
pub async fn faststart_mp4(input: &str, output: &str) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",         // overwrite output
        "-i",
        input,        // input mp4
        "-c",
        "copy",       // remux saja
        "-movflags",
        "+faststart", // pindah 'moov' ke depan
        output,       // output mp4
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn ffmpeg: {e}"))?;

    let mut stderr = child.stderr.take().unwrap();
    let mut err_buf = Vec::new();
    let stderr_task = tokio::spawn(async move {
        let mut tmp = [0u8; 8192];
        while let Ok(n) = stderr.read(&mut tmp).await {
            if n == 0 {
                break;
            }
            err_buf.extend_from_slice(&tmp[..n]);
        }
        err_buf
    });

    let status = child.wait().await.map_err(|e| anyhow!("wait ffmpeg: {e}"))?;
    let err_bytes = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let err_str = String::from_utf8_lossy(&err_bytes);
        return Err(anyhow!(
            "ffmpeg exit code {:?}. stderr:\n{}",
            status.code(),
            err_str
        ));
    }

    Ok(())
}

