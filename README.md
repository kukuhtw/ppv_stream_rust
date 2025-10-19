
````markdown
# 🎬 PPV Stream — Rust + Axum + SQLite + HLS (Pay-Per-View)

Boilerplate aplikasi streaming **Pay-Per-View** dengan autentikasi user & admin, dashboard admin, reset password, serta fondasi streaming HLS bertoken dengan watermark dinamis.

---

## ✨ Fitur Utama
- 🧍 **Autentikasi User & Admin**
  - Registrasi, login, logout, lupa/reset password.
- 🧑‍💼 **Dashboard Admin**
  - Statistik pengguna, video, dan transaksi.
  - Upload video MP4, tetapkan tarif “pay per view”.
- 🎥 **Streaming Aman**
  - Fondasi HLS dengan token & watermark (menggunakan FFmpeg).
- 🧾 **Database**
  - SQLite (default) — mudah diganti ke PostgreSQL di produksi.
- 🛡️ **Keamanan**
  - Token HMAC short-lived untuk setiap sesi pemutaran.
  - Watermark nama user + waktu untuk mencegah pembajakan.

---

## ⚙️ Prasyarat
Pastikan sistem kamu memiliki:

| Komponen | Perintah Cek |
|-----------|----------------|
| **Rust (stable)** | `rustup default stable` |
| **FFmpeg** | `ffmpeg -version` |
| **SQLite** | sudah termasuk dalam `sqlx/sqlite` |
| **Node.js (opsional)** | `node -v` — untuk build UI jika dikembangkan lebih lanjut |

---

## 🚀 Setup & Jalankan

1. **Salin variabel lingkungan:**
   ```bash
   cp .env.example .env
````

2. **Jalankan server (mode dev):**

   ```bash
   cargo run
   ```

3. **Akses di browser:**

   * User:

     * `/public/auth/register.html`
     * `/public/auth/login.html`
   * Admin:

     * `/public/admin/login.html`
     * `/admin/dashboard`
   * Browse:

     * `/public/browse.html`
   * Watch (demo):

     * `/public/watch.html`

---

## ⚙️ Variabel Lingkungan

| Nama                        | Deskripsi                         | Default                                                |
| --------------------------- | --------------------------------- | ------------------------------------------------------ |
| `APP_BIND`                  | Alamat bind server                | `0.0.0.0:3000`                                         |
| `DATABASE_URL`              | URL database SQLite/Postgres      | `sqlite://ppv.db`                                      |
| `HMAC_SECRET`               | Kunci HMAC untuk token streaming  | **ubah di produksi**                                   |
| `SESSION_TOKEN_TTL_SECONDS` | Umur token streaming (detik)      | `600`                                                  |
| `HLS_SEGMENT_SECONDS`       | Durasi tiap segmen HLS (detik)    | `6`                                                    |
| `WATERMARK_FONT`            | Path font untuk watermark FFmpeg  | `/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf` |
| `ADMIN_BOOTSTRAP_EMAIL`     | Email admin pertama (opsional)    | -                                                      |
| `ADMIN_BOOTSTRAP_PASSWORD`  | Password admin pertama (opsional) | -                                                      |

---

## 🧑‍💼 Membuat Admin Pertama

Terdapat dua cara:

### **Opsi A — via `.env`**

Isi variabel berikut:

```bash
ADMIN_BOOTSTRAP_EMAIL=admin@example.com
ADMIN_BOOTSTRAP_PASSWORD=ChangeMe123!
```

Saat server pertama kali dijalankan, user ini otomatis dibuat & di-promote menjadi admin.

### **Opsi B — via SQL manual**

Jika user sudah terdaftar:

```sql
UPDATE users SET is_admin=1 WHERE email='emailmu@contoh.com';
```

---

## 🧱 Struktur Proyek

```
ppv_stream/
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
├── Makefile
├── sql/
│   ├── 001_init.sql
│   ├── 002_admins.sql
│   ├── 003_password_resets.sql
│   └── 004_sessions.sql
├── src/
│   ├── main.rs
│   ├── bootstrap.rs
│   ├── config.rs
│   ├── handlers/
│   │   ├── upload.rs
│   │   ├── stream.rs
│   │   ├── video.rs
│   │   ├── auth_user.rs
│   │   ├── auth_admin.rs
│   │   ├── admin.rs
│   │   ├── setup.rs
│   │   └── password.rs
│   └── ...
└── public/
    ├── auth/
    ├── admin/
    ├── browse.html
    ├── dashboard.html
    └── watch.html
```

---

## 🧠 Alur Singkat Aplikasi

1. **User Registrasi & Login**

   * `/auth/register`, `/auth/login`
   * Data tersimpan di tabel `users`.

2. **Admin Upload Video**

   * `/api/upload`
   * MP4 disimpan di `/storage/<uuid>.mp4`
   * Metadata tersimpan di tabel `videos`.

3. **Menentukan Akses**

   * Admin menambahkan user ke allowlist (`/api/allow`)
     atau user membeli video (`/api/purchase`).

4. **Menonton Video**

   * Frontend memanggil `/api/request_play`
   * Backend cek hak akses → generate segmen HLS + watermark.
   * Video diputar via `hls.js` di browser.

5. **Keamanan**

   * Token HMAC bertenggat waktu melindungi link streaming.
   * Video tidak dapat diunduh karena hanya dilayani via segmen `.m3u8` dan `.ts`.

---

## 🧾 Catatan Produksi

| Area              | Rekomendasi                                   |
| ----------------- | --------------------------------------------- |
| **Database**      | Gunakan PostgreSQL untuk skala besar          |
| **Storage**       | Gunakan S3 / MinIO untuk file video           |
| **Payment**       | Integrasi PSP (Tripay, Midtrans, Stripe)      |
| **Streaming**     | Implementasi Encrypted HLS / Multi-DRM        |
| **CDN**           | Gunakan CloudFront atau Cloudflare            |
| **Keamanan**      | Aktifkan HTTPS, rate-limit, CSRF, dan logging |
| **Observability** | Gunakan Prometheus + Grafana                  |

---

## 📦 Jalankan dengan Docker

```bash
make build     # build image
make run       # jalankan docker compose
make stop      # hentikan container
```

Akses aplikasi:

```
http://localhost:3000/public/auth/login.html
```

---

## 📜 Lisensi

MIT License — gunakan dan kembangkan sesuai kebutuhan proyekmu.

```


```
