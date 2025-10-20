// src/config.rs
use std::{env, fs, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    // ===== Koneksi & server =====
    pub database_url: String,
    pub bind: String,

    // ===== Direktori =====
    pub upload_dir: String,
    pub media_dir: String,
    pub tmp_dir: String,
    pub public_dir: String,

    // ===== Alias lama =====
    pub storage_dir: String,
    pub hls_root: String,

    // ===== Parameter HLS & watermark =====
    pub hls_segment_seconds: u32, // ← hanya sekali, sebagai u32
    pub watermark_font: String,

    // ===== Session & security =====
    pub session_token_ttl: u64,
    pub hmac_secret: Vec<u8>,

    // ===== Hardware acceleration =====
    pub hwaccel: String,

    // ===== Batas upload =====
    pub max_upload_bytes: u64,
}

impl Config {
    pub fn from_env() -> Self {
        // DB & server
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://ppv:secret@localhost:5432/ppv_stream".into());
        let bind = env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());

        // Direktori
        let upload_dir_env = env::var("UPLOAD_DIR").ok();
        let media_dir_env = env::var("MEDIA_DIR").ok();
        let tmp_dir = env::var("TMP_DIR").unwrap_or_else(|_| "/dev/shm/ppv_tmp".into());
        let public_dir = env::var("PUBLIC_DIR")
            .unwrap_or_else(|_| format!("{}/public", env!("CARGO_MANIFEST_DIR")));

        // Alias lama
        let storage_dir = env::var("STORAGE_DIR").unwrap_or_else(|_| "storage".into());
        let hls_root = env::var("HLS_ROOT").unwrap_or_else(|_| "hls_tmp".into());

        let upload_dir = upload_dir_env.unwrap_or_else(|| storage_dir.clone());
        let media_dir = media_dir_env.unwrap_or_else(|| hls_root.clone());

        // Parameter HLS & watermark
        let hls_segment_seconds = env::var("HLS_SEGMENT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(2);
        let watermark_font = env::var("WATERMARK_FONT")
            .unwrap_or_else(|_| "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());

        // Session & security
        let session_token_ttl = env::var("SESSION_TOKEN_TTL")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3600);
        let hmac_secret = env::var("HMAC_SECRET")
            .map(|s| s.into_bytes())
            .unwrap_or_else(|_| b"dev-secret-change-me".to_vec());

        // HW accel & upload limit
        let hwaccel = env::var("HWACCEL").unwrap_or_else(|_| "none".into());
        let max_upload_bytes = env::var("MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1_024 * 1_024 * 1_024);

        let cfg = Self {
            database_url,
            bind,
            upload_dir,
            media_dir,
            tmp_dir,
            public_dir,
            storage_dir,
            hls_root,
            hls_segment_seconds,
            watermark_font,
            session_token_ttl,
            hmac_secret,
            hwaccel,
            max_upload_bytes,
        };

        cfg.ensure_dirs();

        println!(
            "[config] bind={}, db_url={}, upload_dir={}, media_dir={}, tmp_dir={}, public_dir={}, hls_segment={}s, hwaccel={}, max_upload={}MB",
            cfg.bind,
            redacted(&cfg.database_url),
            cfg.upload_dir,
            cfg.media_dir,
            cfg.tmp_dir,
            cfg.public_dir,
            cfg.hls_segment_seconds,
            cfg.hwaccel,
            cfg.max_upload_bytes / (1024 * 1024)
        );

        cfg
    }

    fn ensure_dirs(&self) {
        for d in [
            &self.upload_dir,
            &self.media_dir,
            &self.tmp_dir,
            &self.public_dir,
        ] {
            if let Err(e) = fs::create_dir_all(d) {
                eprintln!("[config] WARNING: gagal membuat dir {}: {}", d, e);
            }
        }
    }

    pub fn video_hls_dir(&self, video_id: &str) -> String {
        let mut p = PathBuf::from(&self.media_dir);
        p.push(video_id);
        p.to_string_lossy().to_string()
    }
}

fn redacted(s: &str) -> String {
    if let Some(idx) = s.find("://") {
        let (scheme, rest) = s.split_at(idx + 3);
        if let Some(at) = rest.find('@') {
            if let Some(colon) = rest[..at].find(':') {
                let user = &rest[..colon];
                let after_at = &rest[at..];
                return format!("{scheme}{user}:***{after_at}");
            }
        }
    }
    s.to_string()
}
