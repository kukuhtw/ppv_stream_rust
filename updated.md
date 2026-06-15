Berikut ringkasan “apa beda logic lama vs yang baru”, fokus ke performa, keamanan, dan alur data.

# 1) Upload video

* **Dulu:** tulis file langsung ke tujuan dengan `File::create` + `write_all` per chunk.
* **Baru:**

  * **Buffered I/O** dengan `BufWriter(≈1MB)` → syscall lebih sedikit.
  * **Tulis ke `*.part` lalu atomic rename** → mencegah file setengah jadi.
  * **Batas ukuran** (`MAX_UPLOAD_BYTES`) + hitung byte real-time.
  * **Whitelist ekstensi** (`ALLOW_EXTS`) + **MIME sniff** best-effort (`infer`).
  * Gagal insert DB ⇒ file dibersihkan.
  * Logging ukuran & lokasi.

# 2) Transcoding worker

* **Dulu:** tidak ada faststart; ABR tidak konsisten/tersebar; beberapa fungsi belum ada.
* **Baru:**

  * **Faststart MP4** (lossless `-c copy -movflags +faststart`) sebelum HLS → seek awal lebih cepat.
  * **ABR multi-rendition dalam 1 proses ffmpeg** (240p/360p/480p) via `-filter_complex` + `-var_stream_map` → efisien CPU/IO.
  * **Anti-upscale** (ladder menyesuaikan tinggi sumber).
  * **Handling audio kosong** (pakai `anullsrc` + `-shortest`).
  * **Concurrency terkendali** dengan `Semaphore`.
  * **Status DB rapi**: `processing → ready|error`, `last_error`, simpan path `hls_master`.
  * **Struktur output bersih di `media/<video_id>`**; `master.m3u8` + varian `v0..`.

# 3) FFMPEG runner & probing

* **Dulu:** `transcode_hls` jalankan argumen mentah, tidak ada “working dir”.
* **Baru:**

  * **`run_ffmpeg(args, work_dir)`** → output relatif menulis ke folder target (rapi & aman).
  * `transcode_hls` sekarang **benar-benar** eksekusi di `session_dir`.
  * Tambahan helper: `ffprobe_duration`, `ffprobe_dimensions`, `ffprobe_has_audio`.
  * **Encode HLS ABR** tersedia sebagai fungsi utilitas yang menghormati `hwaccel` (default CPU).

# 4) Streaming (play) & serving HLS

* **Dulu:** baca file HLS ke mem (byte vector) lalu kirim; watermark on-the-fly serupa.
* **Baru:**

  * **Streaming file** pakai `ReaderStream` → tidak load seluruh file ke RAM.
  * Header **`Cache-Control: no-store`** konsisten.
  * Validasi path/ekstensi lebih ketat.
  * Watermark tetap bergerak; **threads** ffmpeg diset ke `num_cpus()`.

# 5) Sessions & cookie

* **Dulu:** cookie simpan `sid` polos; TTL fix 7 hari; tanpa integritas.
* **Baru:**

  * **TTL dari config** (`SESSION_TOKEN_TTL`).
  * **Cookie ditandatangani HMAC-SHA256** (format `b64(sid).b64(sig)`) → anti pemalsuan.
  * Atribut cookie aman: `HttpOnly`, `SameSite=Lax`.
  * **API berubah:** `create_session/destroy_session/current_user_id` sekarang butuh `&Config` (untuk `hmac_secret` & TTL).

# 6) Konfigurasi & direktori

* **Dulu:** `media_dir` kadang default ke `hls_root`, `tmp_dir` fix; tidak ada `allow_exts`.
* **Baru:**

  * **`media_dir` default `media/`**, **`hls_root`** khusus session HLS on-the-fly.
  * **`tmp_dir`** cross-platform (pakai OS temp, fallback `/dev/shm` di Linux bila ada).
  * **`ensure_dirs`** juga membuat `hls_root`.
  * **`allow_exts: Vec<String>`** dari `ALLOW_EXTS`.
  * Log startup meredaksi kredensial DB.

# 7) Keamanan & robustness

* **Dulu:** kemungkinan race/partial file saat upload; cookie bisa dipalsukan; serving baca penuh.
* **Baru:**

  * **Atomic rename** + size limit + MIME check pada upload.
  * **HMAC cookie** + cleanup session expired.
  * **Streaming I/O** untuk HLS.
  * Error path menulis **`last_error`** di DB untuk diagnosa.

# 8) Dampak migrasi (API yang berubah)

* `sessions::*` sekarang:

  * `create_session(pool, &cfg, user_id, is_admin, cookies)`
  * `destroy_session(pool, &cfg, cookies)`
  * `current_user_id(pool, &cfg, cookies)`
* Worker: `Worker::new(pool, cfg, concurrency)` (menyimpan cfg untuk TTL/dir).
* `ffmpeg::run_ffmpeg(args, work_dir)` dipakai di worker & streaming.
* **ENV baru/terpakai:** `ALLOW_EXTS`, `MAX_UPLOAD_BYTES`, `SESSION_TOKEN_TTL`, `HMAC_SECRET`, `HLS_ROOT`, `MEDIA_DIR`, `TMP_DIR`, `WATERMARK_FONT`.

---

## Intinya

Versi baru lebih **cepat** (buffered I/O, multi-rendition dalam satu proses, faststart), lebih **hemat memori** (streaming HLS), dan jauh lebih **aman** (HMAC cookie, path validation, size & MIME checks), plus **observabilitas** (status DB + `last_error`).
