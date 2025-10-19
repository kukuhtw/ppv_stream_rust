// src/hls.rs

/// Susun argumen ffmpeg untuk output HLS dengan watermark teks (username)
pub fn ffmpeg_hls_args(
    input_path: &str,
    out_dir: &str,
    segment_seconds: u32,
    font_path: &str,
    watermark_text: &str,
) -> Vec<String> {
    // File output
    let playlist = format!("{}/index.m3u8", out_dir);
    let segment_fmt = format!("{}/segment_%04d.ts", out_dir);

    // Filter drawtext: watermark semi-transparan di pojok kanan atas
    // NB: karakter ':' dan '\' dalam text perlu di-escape untuk ffmpeg drawtext.
    let escaped = watermark_text
        .replace('\\', "\\\\")
        .replace(':', "\\:");

    let drawtext = format!(
        "drawtext=fontfile={}:text='{}':x=w-tw-20:y=20:fontsize=24:fontcolor=white@0.6:borderw=2:bordercolor=black@0.6",
        font_path,
        escaped,
    );

    vec![
        "-y".into(),                    // overwrite tanpa tanya
        "-i".into(), input_path.into(),// input
        "-vf".into(), drawtext,         // watermark
        "-c:v".into(), "libx264".into(),
        "-preset".into(), "veryfast".into(),
        "-crf".into(), "23".into(),
        "-c:a".into(), "aac".into(),
        "-b:a".into(), "128k".into(),
        "-hls_time".into(), segment_seconds.to_string(),
        "-hls_playlist_type".into(), "event".into(),
        "-hls_segment_filename".into(), segment_fmt,
        "-f".into(), "hls".into(),
        playlist,
    ]
}
