// src/config.rs

use std::env;

/// Struktur konfigurasi global untuk aplikasi.
/// Semua nilai diambil dari environment variable, dengan default fallback.
#[derive(Clone)]
pub struct Config {
    pub storage_dir: String,           // Folder tempat menyimpan file video hasil upload
    pub hls_root: String,              // Folder sementara untuk hasil segmentasi HLS
    pub hls_segment_seconds: u32,      // Durasi tiap segmen HLS (detik)
    pub watermark_font: String,        // Path font untuk watermark teks di video
    pub session_token_ttl: u64,        // Waktu hidup session token (detik)
    pub hmac_secret: Vec<u8>,          // Secret key untuk HMAC sign/verify
    pub database_url: String,          // URL koneksi database Postgres
}

impl Config {
    /// Membaca konfigurasi dari environment variables dengan fallback default.
    pub fn from_env() -> Self {
        // Ambil variabel dari environment, atau fallback ke default
        let storage_dir = env::var("STORAGE_DIR").unwrap_or_else(|_| "storage".into());
        let hls_root = env::var("HLS_ROOT").unwrap_or_else(|_| "hls_tmp".into());
        let hls_segment_seconds = env::var("HLS_SEGMENT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(6);
        let watermark_font = env::var("WATERMARK_FONT")
            .unwrap_or_else(|_| "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
        let session_token_ttl = env::var("SESSION_TOKEN_TTL")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3600);
        let hmac_secret = env::var("HMAC_SECRET")
            .map(|s| s.into_bytes())
            .unwrap_or_else(|_| b"dev-secret-change-me".to_vec());
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://ppv:secret@localhost:5432/ppv_stream".into());

        let cfg = Self {
            storage_dir,
            hls_root,
            hls_segment_seconds,
            watermark_font,
            session_token_ttl,
            hmac_secret,
            database_url,
        };

        // Cetak info penting saat aplikasi start (non-sensitif)
        println!(
            "[config] storage_dir={}, hls_root={}, hls_segment_seconds={}s, db_url={}",
            cfg.storage_dir, cfg.hls_root, cfg.hls_segment_seconds, cfg.database_url
        );

        cfg
    }
}
