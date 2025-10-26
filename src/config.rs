// src/config.rs
// src/config.rs
use std::{env, fs, path::PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    // ===== Koneksi & server =====
    pub database_url: String,
    pub bind: String,

    // ===== Direktori =====
    pub upload_dir: String,   // lokasi file asli hasil upload
    pub media_dir: String,    // lokasi hasil transcode VOD (HLS siap)
    pub tmp_dir: String,      // lokasi file sementara
    pub public_dir: String,   // lokasi static/public (opsional)

    // ===== Alias lama / compat =====
    pub storage_dir: String,  // alias lama untuk upload_dir
    pub hls_root: String,     // root session HLS on-the-fly (stream.rs)

    // ===== Parameter HLS & watermark =====
    pub hls_segment_seconds: u32,
    pub watermark_font: String,

    // ===== Session & security =====
    pub session_token_ttl: u64,
    pub hmac_secret: Vec<u8>,

    // ===== Hardware acceleration (opsional) =====
    pub hwaccel: String,

    // ===== Batas upload =====
    pub max_upload_bytes: u64,
    pub allow_exts: Vec<String>, // e.g. ["mp4","mkv","mov","webm"]

    // ===== Kurs Dollar ke Rupiah =====
    pub dollar_usd_to_rupiah: f64,

    // ===== X402 =====
    pub x402_contract: String,
    pub x402_admin_wallet: String,
    pub x402_rpc_wss: String, // untuk watcher (WebSocket RPC)
    pub x402_chain_id: u64,   // chain default (mis: 137)
}

impl Config {
    pub fn from_env() -> Self {
        // DB & server
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://ppv:secret@localhost:5432/ppv_stream".into());
        let bind = env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());

        // Direktori
        let storage_dir = env::var("STORAGE_DIR").unwrap_or_else(|_| "storage".into());
        let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| storage_dir.clone());

        // Jangan jadikan media_dir default ke hls_root (membingungkan).
        // Pakai "media" agar VOD hasil transcode punya rumah jelas.
        let media_dir = env::var("MEDIA_DIR").unwrap_or_else(|_| "media".into());

        // hls_root = direktori session HLS (on-the-fly, sekali pakai)
        let hls_root = env::var("HLS_ROOT").unwrap_or_else(|_| "hls_tmp".into());

        // tmp_dir cross-platform: default ke OS temp; fallback Linux shm untuk kecepatan
        let tmp_dir = env::var("TMP_DIR").unwrap_or_else(|_| {
            let mut t = env::temp_dir();
            if cfg!(target_os = "linux") {
                // jika ada /dev/shm, gunakan itu
                let shm = PathBuf::from("/dev/shm/ppv_tmp");
                if shm.parent().unwrap_or(&PathBuf::new()).exists() {
                    return shm.to_string_lossy().to_string();
                }
            }
            t.push("ppv_tmp");
            t.to_string_lossy().to_string()
        });

        // public_dir default ke <project>/public
        let public_dir = env::var("PUBLIC_DIR")
            .unwrap_or_else(|_| format!("{}/public", env!("CARGO_MANIFEST_DIR")));

        // Parameter HLS & watermark
        let hls_segment_seconds = env::var("HLS_SEGMENT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(2);
        let watermark_font = env::var("WATERMARK_FONT").unwrap_or_else(|_| {
            // default font yang umum ada di Linux; di Windows/Mac sebaiknya set via env
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()
        });

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
            .unwrap_or(1_024 * 1_024 * 1_024); // 1 GiB

        // Whitelist ekstensi untuk upload
        let allow_exts = parse_csv_list(env::var("ALLOW_EXTS").unwrap_or_else(|_| {
            // default aman umum
            "mp4,mkv,mov,webm".into()
        }));

        // Kurs Dollar ke Rupiah (default 17000)
        let dollar_usd_to_rupiah = env::var("DOLLAR_USD_TO_RUPIAH")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(17_000.0);

        // ===== X402 =====
        let x402_contract = env::var("X402_CONTRACT_ADDRESS").unwrap_or_default();
        let x402_admin_wallet = env::var("X402_ADMIN_WALLET").unwrap_or_default();
        let x402_rpc_wss = env::var("X402_RPC_WSS").unwrap_or_default();
        let x402_chain_id = env::var("X402_CHAIN_ID")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

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
            allow_exts,
            dollar_usd_to_rupiah,
            x402_contract,
            x402_admin_wallet,
            x402_rpc_wss,
            x402_chain_id,
        };

        cfg.ensure_dirs();

        println!(
            "[config] bind={}, db_url={}, upload_dir={}, media_dir={}, tmp_dir={}, public_dir={}, \
             hls_segment={}s, hwaccel={}, kurs_usd_to_idr={}, max_upload={}MB, \
             x402_contract={}, x402_chain_id={}, watcher_wss={}",
            cfg.bind,
            redacted(&cfg.database_url),
            cfg.upload_dir,
            cfg.media_dir,
            cfg.tmp_dir,
            cfg.public_dir,
            cfg.hls_segment_seconds,
            cfg.hwaccel,
            cfg.dollar_usd_to_rupiah,
            cfg.max_upload_bytes / (1024 * 1024),
            if cfg.x402_contract.is_empty() { "-" } else { &cfg.x402_contract },
            cfg.x402_chain_id,
            if cfg.x402_rpc_wss.is_empty() { "-" } else { "set" }
        );

        cfg
    }

    fn ensure_dirs(&self) {
        for d in [
            &self.upload_dir,
            &self.media_dir,
            &self.tmp_dir,
            &self.public_dir,
            &self.hls_root, // <-- jangan lupa bikin session root juga
        ] {
            if let Err(e) = fs::create_dir_all(d) {
                eprintln!("[config] WARNING: gagal membuat dir {}: {}", d, e);
            }
        }
    }

    /// Lokasi hasil transcode VOD (HLS siap) untuk video tertentu
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

fn parse_csv_list(s: String) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().trim_matches(&[' ', '.', ';'][..]).to_ascii_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}
