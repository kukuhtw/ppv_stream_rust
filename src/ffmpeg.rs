// src/ffmpeg.rs

use anyhow::{anyhow, Context, Result};
use std::{path::Path, process::Stdio};
use tokio::{io::AsyncReadExt, process::Command};

/// Runner generik untuk menjalankan `ffmpeg` dengan argumen yang diberikan.
/// Mengumpulkan stderr agar mudah di-debug saat gagal.
async fn run_ffmpeg(args: Vec<String>) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn ffmpeg: {e}"))?;

    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to take ffmpeg stderr"))?;
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

    let status = child
        .wait()
        .await
        .map_err(|e| anyhow!("wait ffmpeg: {e}"))?;

    let err_bytes = stderr_task.await.unwrap_or_default();

    if !status.success() {
        let err_str = String::from_utf8_lossy(&err_bytes);
        return Err(anyhow!(
            "ffmpeg exited with code {:?}\nargs: {}\nstderr:\n{}",
            status.code(),
            args.join(" "),
            err_str
        ));
    }

    Ok(())
}

/// Jalankan ffmpeg async dengan argumen lengkap dari pemanggil (kompatibel dengan versi lama).
/// Dipakai bila perlu pipeline kustom.
pub async fn transcode_hls(_input_path: &str, _session_dir: &str, args: Vec<String>) -> Result<()> {
    run_ffmpeg(args).await
}

/// Remux MP4 agar `moov` dipindah ke awal file (progressive playback).
/// - Lossless & cepat (pakai `-c copy`)
/// - Aman dipakai setelah upload sebelum file disegment untuk HLS
pub async fn faststart_mp4(input: &str, output: &str) -> Result<()> {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.into(),
    ];
    run_ffmpeg(args).await
}

/// (Opsional) Ambil durasi (detik) via ffprobe. Kembalikan None bila gagal.
pub async fn ffprobe_duration(input: &str) -> Option<f64> {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v", "error",
        "-show_entries", "format=duration",
        "-of", "default=noprint_wrappers=1:nokey=1",
        input,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            let mut out = Vec::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut out).await;
            }
            let _ = child.wait().await;
            let s = String::from_utf8_lossy(&out).trim().to_string();
            s.parse::<f64>().ok()
        }
        Err(_) => None,
    }
}

/// Ambil dimensi video (width, height) via ffprobe. Kembalikan None bila gagal.
pub async fn ffprobe_dimensions(input: &str) -> Option<(u32, u32)> {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v", "error",
        "-select_streams", "v:0",
        "-show_entries", "stream=width,height",
        "-of", "csv=p=0:s=x",
        input,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            let mut out = Vec::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut out).await;
            }
            let _ = child.wait().await;
            let s = String::from_utf8_lossy(&out).trim().to_string(); // "WIDTHxHEIGHT"
            let mut it = s.split('x');
            let w = it.next()?.parse::<u32>().ok()?;
            let h = it.next()?.parse::<u32>().ok()?;
            Some((w, h))
        }
        Err(_) => None,
    }
}

/// Cek apakah ada stream audio.
pub async fn ffprobe_has_audio(input: &str) -> bool {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v", "error",
        "-select_streams", "a:0",
        "-show_entries", "stream=index",
        "-of", "csv=p=0",
        input,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            let mut out = Vec::new();
            if let Some(mut stdout) = child.stdout.take() {
                let _ = stdout.read_to_end(&mut out).await;
            }
            let _ = child.wait().await;
            !String::from_utf8_lossy(&out).trim().is_empty()
        }
        Err(_) => false,
    }
}

/// Encode HLS ABR (1080p/720p/480p) TANPA watermark.
/// - `hwaccel`: "none" | "nvidia" | "intel" | "amd"
/// - `seg_seconds`: durasi setiap segmen HLS
/// Return: path absolut `master.m3u8` (String)
pub async fn encode_hls_abr(
    input_mp4: &str,
    out_dir: &str,
    hwaccel: &str,
    seg_seconds: u32,
) -> Result<String> {
    // Pastikan output dir ada
    tokio::fs::create_dir_all(out_dir)
        .await
        .with_context(|| format!("create_dir_all({out_dir})"))?;

    // Master playlist path
    let master = Path::new(out_dir).join("master.m3u8");
    let master_str = master
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 path for master.m3u8"))?
        .to_string();

    // === Anti-upscale: tentukan ladder dari tinggi sumber ===
    let source_h = match ffprobe_dimensions(input_mp4).await {
        Some((_w, h)) => h,
        None => 1080, // fallback konservatif
    };

    // Default ladder tinggi (descending). Hanya pilih yang ≤ source_h.
    let mut ladder: Vec<u32> = vec![1080, 720, 480]
        .into_iter()
        .filter(|&h| h <= source_h)
        .collect();

    // Safety: minimal 1 varian
    if ladder.is_empty() {
        // kalau sumber < 480, pakai tinggi sumber dibulatkan ke kelipatan 2 terdekat (FFmpeg syarat even)
        let safe_h = (source_h / 2).max(1) * 2;
        ladder.push(safe_h);
    }

    // fps * seg_seconds ≈ GOP size; fallback 24 fps
    let g = (24 * seg_seconds.max(1)) as i32;
    let seg_str = seg_seconds.to_string();

    // Apakah ada audio asli?
    let has_audio = ffprobe_has_audio(input_mp4).await;

    // === Filter: split sebanyak N, scale tiap varian (tanpa drawtext) ===
    // [0:v]split=N[v0][v1]...[vN-1]; [v0]scale=-2:H0[vout0]; ...
    let n = ladder.len();
    let split_labels: Vec<String> = (0..n).map(|i| format!("[v{i}]")).collect();
    let vouts: Vec<String> = (0..n).map(|i| format!("[vout{i}]")).collect();

    let split_part = format!(
        "[0:v]split={}{labels}",
        n,
        labels = split_labels.join("")
    );

    let scale_parts: Vec<String> = ladder
        .iter()
        .enumerate()
        .map(|(i, h)| format!("[v{i}]scale=-2:{h}[vout{i}]"))
        .collect();

    let filter_complex = format!("{};{}", split_part, scale_parts.join(";"));

    // === Base args & input(s) ===
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        // input video utama
        "-i".into(),
        input_mp4.into(),
        // graph filter
        "-filter_complex".into(),
        filter_complex,
    ];

    // Jika TIDAK ada audio, tambahkan input anullsrc sebagai input ke-2 (index 1)
    // anullsrc "tak terbatas"; gunakan -shortest agar mux berhenti saat video selesai.
    let audio_map_src = if has_audio {
        "0:a:0".to_string()
    } else {
        args.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            "anullsrc=channel_layout=stereo:sample_rate=48000".into(),
        ]);
        "1:a:0".to_string()
    };

    // Map setiap varian video + audio (asli atau anullsrc)
    for i in 0..n {
        args.push("-map".into());
        args.push(vouts[i].clone());     // video i
        args.push("-map".into());
        args.push(audio_map_src.clone()); // audio
    }

    // Audio settings (berlaku untuk semua audio output)
    args.extend(
        [
            "-c:a", "aac",
            "-b:a", "128k",
            "-ac", "2",
            "-ar", "48000",
        ]
        .into_iter()
        .map(Into::into),
    );

    // Video encoder per stream sesuai hwaccel
    match hwaccel {
        "nvidia" => {
            // NVENC
            for i in 0..n {
                let cq = 22 + (i as i32); // 22,23,24...
                args.extend(
                    [
                        format!("-c:v:{i}"), "h264_nvenc".into(),
                        format!("-preset:v:{i}"), "p1".into(),
                        format!("-rc:v:{i}"), "vbr".into(),
                        format!("-cq:v:{i}"), cq.to_string(),
                    ]
                );
            }
        }
        "intel" => {
            // QuickSync
            for i in 0..n {
                let gq = 23 + (i as i32);
                args.extend(
                    [
                        format!("-c:v:{i}"), "h264_qsv".into(),
                        format!("-global_quality:v:{i}"), gq.to_string(),
                    ]
                );
            }
        }
        "amd" => {
            // VAAPI
            for i in 0..n {
                let qp = 23 + (i as i32);
                args.extend(
                    [
                        format!("-c:v:{i}"), "h264_vaapi".into(),
                        format!("-qp:v:{i}"), qp.to_string(),
                    ]
                );
            }
        }
        _ => {
            // CPU libx264
            for i in 0..n {
                let crf = 22 + (i as i32);
                args.extend(
                    [
                        format!("-c:v:{i}"), "libx264".into(),
                        format!("-preset:v:{i}"), "veryfast".into(),
                        format!("-crf:v:{i}"), crf.to_string(),
                    ]
                );
            }
        }
    };

    // GOP & scene-cut per varian
    for i in 0..n {
        args.extend(
            [
                format!("-g:v:{i}"), g.to_string(),
                format!("-keyint_min:v:{i}"), g.to_string(),
                format!("-sc_threshold:v:{i}"), "0".into(),
            ]
        );
    }

    // -shortest perlu jika pakai anullsrc (tanpa -t)
    if !has_audio {
        args.push("-shortest".into());
    }

    // HLS output (independent seg + temp_file)
    // Bangun var_stream_map dinamis: v:0,a:0 v:1,a:1 ...
    let var_stream_map = (0..n)
        .map(|i| format!("v:{i},a:{i}"))
        .collect::<Vec<_>>()
        .join(" ");

    args.extend(
        [
            "-f", "hls",
            "-hls_time", &seg_str,
            "-hls_playlist_type", "vod",
            "-hls_flags", "independent_segments+temp_file",
            "-master_pl_name", "master.m3u8",
            "-var_stream_map", &var_stream_map,
            "-hls_segment_filename", &format!("{}/stream_%v_%06d.ts", out_dir),
            &format!("{}/stream_%v.m3u8", out_dir),
        ]
        .into_iter()
        .map(Into::into),
    );

    run_ffmpeg(args).await?;
    Ok(master_str)
}
