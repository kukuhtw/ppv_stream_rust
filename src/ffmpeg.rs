// src/ffmpeg.rs
//
// FFmpeg and FFprobe integration utilities used by the video processing pipeline.
//
// This module is responsible for:
// 1. Executing FFmpeg commands asynchronously.
// 2. Capturing FFmpeg diagnostic output when processing fails.
// 3. Optimizing MP4 files for progressive playback.
// 4. Reading media metadata through FFprobe.
// 5. Detecting source resolution and audio availability.
// 6. Producing adaptive bitrate HLS output for video streaming.
// 7. Selecting CPU or hardware accelerated H.264 encoders.

use anyhow::{anyhow, Context, Result};
use std::{path::Path, process::Stdio};
use tokio::{io::AsyncReadExt, process::Command};

/// Executes FFmpeg inside a specified working directory.
///
/// Arguments:
/// * `args` contains FFmpeg command arguments without the FFmpeg binary name.
/// * `work_dir` is used as the process working directory so relative output
///   paths are written to the expected location.
///
/// The function captures FFmpeg standard error output and returns it as part
/// of the error message when the FFmpeg process exits unsuccessfully.
pub async fn run_ffmpeg(args: &[String], work_dir: &str) -> Result<()> {
    // Create the FFmpeg child process and prevent it from requesting
    // interactive input from the application runtime.
    let mut cmd = Command::new("ffmpeg");
    cmd.current_dir(work_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn ffmpeg: {e}"))?;

    // Read standard error concurrently. FFmpeg normally writes progress and
    // diagnostics to stderr, so consuming it prevents the pipe from filling
    // and preserves useful failure details for troubleshooting.
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to take ffmpeg stderr"))?;
    let stderr_task = tokio::spawn(async move {
        let mut err_buf = Vec::new();
        let mut tmp = [0u8; 8192];
        while let Ok(n) = stderr.read(&mut tmp).await {
            if n == 0 {
                break;
            }
            err_buf.extend_from_slice(&tmp[..n]);
        }
        err_buf
    });

    // Wait for FFmpeg to complete before evaluating its exit status.
    let status = child
        .wait()
        .await
        .map_err(|e| anyhow!("wait ffmpeg: {e}"))?;

    let err_bytes = stderr_task.await.unwrap_or_default();

    // Return the command arguments and FFmpeg diagnostics when transcoding
    // fails so the caller has enough information to investigate the problem.
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

/// Compatibility wrapper retained for older handlers.
///
/// It executes the supplied FFmpeg arguments inside `session_dir`, allowing
/// relative playlist and segment paths to be written into that session folder.
/// The `_input_path` parameter is retained only to preserve the older function
/// signature and is not currently used.
pub async fn transcode_hls(_input_path: &str, session_dir: &str, args: Vec<String>) -> Result<()> {
    run_ffmpeg(&args, session_dir).await
}

/// Remuxes an MP4 file with the `moov` metadata atom placed near the beginning.
///
/// This enables progressive playback because clients can read the MP4 metadata
/// before the entire file has been downloaded. Stream copy is used, so the
/// operation is fast and does not re-encode or reduce media quality.
pub async fn faststart_mp4(input: &str, output: &str) -> Result<()> {
    // Run FFmpeg in the output file's parent directory so the target file can
    // be passed as a relative name and written directly into that directory.
    let work_dir = Path::new(output)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_string_lossy()
        .to_string();

    // Extract a valid UTF-8 file name for use as the relative FFmpeg target.
    let target = Path::new(output)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("non-utf8 output file name"))?
        .to_string();

    // Copy every stream without re-encoding and move the MP4 metadata atom
    // to the beginning of the output file.
    let args = vec![
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
        target,
    ];

    run_ffmpeg(&args, &work_dir).await
}

/// Reads the media duration in seconds through FFprobe.
///
/// Returns `None` when FFprobe cannot be started, produces invalid output,
/// or the duration cannot be parsed as a floating point number.
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
            // Capture the machine-readable duration value from FFprobe stdout.
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

/// Reads the first video stream dimensions through FFprobe.
///
/// Returns `(width, height)` when metadata is available, or `None` when the
/// probe fails or the output cannot be parsed.
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

            // FFprobe returns dimensions in the form `WIDTHxHEIGHT`.
            let s = String::from_utf8_lossy(&out).trim().to_string();
            let mut it = s.split('x');
            let w = it.next()?.parse::<u32>().ok()?;
            let h = it.next()?.parse::<u32>().ok()?;
            Some((w, h))
        }
        Err(_) => None,
    }
}

/// Checks whether the first audio stream exists in the source media.
///
/// HLS variants are expected to contain audio tracks. When this function
/// returns `false`, the encoder later injects a silent stereo audio source so
/// all variants remain structurally consistent.
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

/// Encodes an MP4 source into adaptive bitrate HLS output.
///
/// Arguments:
/// * `input_mp4` is the source MP4 file.
/// * `out_dir` is the directory that receives playlists and transport segments.
/// * `hwaccel` selects `none`, `nvidia`, `intel`, or `amd` encoding.
/// * `seg_seconds` controls the approximate duration of each HLS segment.
///
/// The output ladder avoids upscaling. Only resolutions less than or equal to
/// the source height are generated. The function returns the absolute path to
/// the generated `master.m3u8` playlist.
pub async fn encode_hls_abr(
    input_mp4: &str,
    out_dir: &str,
    hwaccel: &str,
    seg_seconds: u32,
) -> Result<String> {
    // Ensure the HLS output directory exists before FFmpeg starts writing files.
    tokio::fs::create_dir_all(out_dir)
        .await
        .with_context(|| format!("create_dir_all({out_dir})"))?;

    // Inspect the source height so the output ladder never enlarges the video.
    // A 1080p fallback is used when the source dimensions cannot be detected.
    let source_h = match ffprobe_dimensions(input_mp4).await {
        Some((_w, h)) => h,
        None => 1080,
    };

    // Build the standard adaptive bitrate ladder in descending order and retain
    // only variants that do not exceed the source resolution.
    let mut ladder: Vec<u32> = vec![1080, 720, 480]
        .into_iter()
        .filter(|&h| h <= source_h)
        .collect();

    // Very small source videos may fall below every standard ladder entry.
    // In that case, preserve a safe even height close to the source height.
    if ladder.is_empty() {
        let safe_h = (source_h / 2).max(1) * 2;
        ladder.push(safe_h);
    }

    let n = ladder.len();

    // Estimate a GOP size for a 24 fps source so keyframes align approximately
    // with HLS segment boundaries. A minimum one-second segment is enforced.
    let g = (24 * seg_seconds.max(1)) as i32;
    let seg_str = seg_seconds.to_string();

    // Build a filter graph that splits the input video into one branch for each
    // variant and scales every branch while preserving the original aspect ratio.
    // Example:
    // [0:v]split=3[v0][v1][v2];
    // [v0]scale=-2:1080...[vout0];
    // [v1]scale=-2:720...[vout1]
    let split_labels: Vec<String> = (0..n).map(|i| format!("[v{i}]")) .collect();
    let vouts: Vec<String> = (0..n).map(|i| format!("[vout{i}]")) .collect();
    let split_part = format!("[0:v]split={}{labels}", n, labels = split_labels.join(""));
    let scale_parts: Vec<String> = ladder
        .iter()
        .enumerate()
        .map(|(i, h)| {
            format!(
                "[v{i}]scale=-2:{h}:force_original_aspect_ratio=decrease:eval=frame[vout{i}]"
            )
        })
        .collect();
    let filter_complex = format!("{};{}", split_part, scale_parts.join(";"));

    // Detect whether the source contains audio before preparing stream mappings.
    let has_audio = ffprobe_has_audio(input_mp4).await;

    // Start constructing the FFmpeg command with the source input and video
    // filter graph used to generate every adaptive bitrate variant.
    let mut args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input_mp4.into(),
        "-filter_complex".into(),
        filter_complex,
    ];

    // Use the source audio stream when present. Otherwise add a silent stereo
    // source at 48 kHz so every HLS variant has a valid audio stream.
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

    // Map one scaled video output and one audio stream for each HLS variant.
    for i in 0..n {
        args.push("-map".into());
        args.push(vouts[i].clone());
        args.push("-map".into());
        args.push(audio_map_src.clone());
    }

    // Encode all audio outputs as stereo AAC at 128 kbps and 48 kHz.
    args.extend(
        ["-c:a", "aac", "-b:a", "128k", "-ac", "2", "-ar", "48000"]
            .into_iter()
            .map(Into::into),
    );

    // Select the H.264 video encoder for every variant.
    // Hardware paths require compatible drivers and an FFmpeg build containing
    // the corresponding encoder. CPU encoding with libx264 is the default.
    match hwaccel {
        "nvidia" => {
            // NVIDIA NVENC uses variable bitrate mode with a quality target.
            for i in 0..n {
                let cq = 22 + (i as i32);
                args.extend([
                    format!("-c:v:{i}"),
                    "h264_nvenc".into(),
                    format!("-preset:v:{i}"),
                    "p1".into(),
                    format!("-rc:v:{i}"),
                    "vbr".into(),
                    format!("-cq:v:{i}"),
                    cq.to_string(),
                ]);
            }
        }
        "intel" => {
            // Intel Quick Sync uses a global quality value for each variant.
            for i in 0..n {
                let gq = 23 + (i as i32);
                args.extend([
                    format!("-c:v:{i}"),
                    "h264_qsv".into(),
                    format!("-global_quality:v:{i}"),
                    gq.to_string(),
                ]);
            }
        }
        "amd" => {
            // AMD or VAAPI encoding uses a fixed quantizer for each variant.
            for i in 0..n {
                let qp = 23 + (i as i32);
                args.extend([
                    format!("-c:v:{i}"),
                    "h264_vaapi".into(),
                    format!("-qp:v:{i}"),
                    qp.to_string(),
                ]);
            }
        }
        _ => {
            // CPU encoding uses libx264 with a fast preset and a CRF quality
            // value that increases slightly for lower-priority variants.
            for i in 0..n {
                let crf = 22 + (i as i32);
                args.extend([
                    format!("-c:v:{i}"),
                    "libx264".into(),
                    format!("-preset:v:{i}"),
                    "veryfast".into(),
                    format!("-crf:v:{i}"),
                    crf.to_string(),
                ]);
            }
        }
    }

    // Configure a consistent GOP structure for all variants. Disabling scene-cut
    // keyframes helps align segment boundaries across the adaptive bitrate ladder.
    for i in 0..n {
        args.extend([
            format!("-g:v:{i}"),
            g.to_string(),
            format!("-keyint_min:v:{i}"),
            g.to_string(),
            format!("-sc_threshold:v:{i}"),
            "0".into(),
        ]);
    }

    // Stop encoding when the real video stream ends if a generated silent audio
    // source is being used, otherwise the synthetic audio could continue forever.
    if !has_audio {
        args.push("-shortest".into());
    }

    // Describe how each video and audio output pair maps to an HLS variant.
    let var_stream_map = (0..n)
        .map(|i| format!("v:{i},a:{i}"))
        .collect::<Vec<_>>()
        .join(" ");

    // Configure video-on-demand HLS output. FFmpeg creates one media playlist
    // per variant, transport stream segments, and a master adaptive playlist.
    args.extend(
        [
            "-f",
            "hls",
            "-hls_time",
            &seg_str,
            "-hls_playlist_type",
            "vod",
            "-hls_flags",
            "independent_segments+temp_file",
            "-master_pl_name",
            "master.m3u8",
            "-var_stream_map",
            &var_stream_map,
            "-hls_segment_filename",
            "stream_%v_%06d.ts",
            "stream_%v.m3u8",
        ]
        .into_iter()
        .map(Into::into),
    );

    // Run FFmpeg inside the output directory so every relative playlist and
    // segment path is generated beneath that directory.
    run_ffmpeg(&args, out_dir).await?;

    // Return the generated master playlist path to the caller.
    let master_abs = Path::new(out_dir).join("master.m3u8");
    let master_str = master_abs
        .to_str()
        .ok_or_else(|| anyhow!("non-utf8 path for master.m3u8"))?
        .to_string();

    Ok(master_str)
}
