# Deploy PPV Stream ke Dokploy

File terkait:

| File | Fungsi |
|---|---|
| `Dockerfile` | Multi-stage build. Builder menjalankan PostgreSQL sementara agar macro `sqlx::query!` bisa diverifikasi tanpa DB eksternal. |
| `docker-compose.dokploy.yml` | Stack `db` + `app` (+ `watcher` opsional) tanpa port publik; trafik lewat Traefik Dokploy. |
| `.env.dokploy.example` | Template variabel untuk tab **Environment** Dokploy. |

## 1. Prasyarat

- Server dengan Dokploy terpasang (min. 2 vCPU / 4 GB RAM; disarankan 4 vCPU / 8 GB karena FFmpeg).
- Domain dengan DNS `A` record ke IP server Dokploy, misal `stream.example.com`.
- Repo `https://github.com/kukuhtw/ppv_stream_rust` terhubung ke Dokploy (GitHub provider atau Git URL).

## 2. Buat service

1. Dokploy → **Projects** → *Create Project* `ppv-stream`.
2. **Create Service → Compose**.
3. Tab **General**:
   - Source: GitHub / Git, branch `main`.
   - Compose Path: `./docker-compose.dokploy.yml`
   - Compose Type: `docker-compose`.
4. Tab **Environment**: tempel isi `.env.dokploy.example`, lalu ganti semua `CHANGE_ME_*`.
   ```bash
   openssl rand -hex 32   # HMAC_SECRET
   openssl rand -hex 24   # POSTGRES_PASSWORD, ADMIN_BOOTSTRAP_TOKEN
   ```
   Dokploy menulis isi tab ini ke `.env` di folder compose, yang dibaca oleh `env_file: .env`.
5. Tab **Domains** → *Add Domain*:
   - Service Name: `app`
   - Host: `stream.example.com`
   - Container Port: `8080`
   - HTTPS: on, Certificate: Let's Encrypt
6. Klik **Deploy**. Build pertama ±10–20 menit (kompilasi Rust); build berikutnya lebih cepat karena cache BuildKit.

## 3. Setelah deploy

- Cek `https://stream.example.com/health` → `ok`.
- Saat start, app otomatis menjalankan:
  1. `sqlx::migrate!("./sql")` (tercatat di `_sqlx_migrations`)
  2. File di `migrations/` (tercatat di `_ppv_extra_migrations`, dijalankan sekali, dengan advisory lock).
     Nonaktifkan dengan `APPLY_EXTRA_MIGRATIONS=0` bila DB lama sudah dimigrasi manual dan ada konflik.
- Buat admin pertama memakai `ADMIN_BOOTSTRAP_*` (lihat `ADMIN_AUTHENTICATION.md`), lalu **rotasi / kosongkan** `ADMIN_BOOTSTRAP_TOKEN` dan `ADMIN_BOOTSTRAP_PASSWORD`, kemudian redeploy.
- Daftarkan URL webhook di dashboard payment provider memakai `BASE_URL` (lihat `PAYMENT.md`).

### Env tambahan (Nov 2026)

| Env | Fungsi |
|---|---|
| `WALLET_DEPOSIT_INSTRUCTIONS` | Rekening tujuan top up yang tampil di `wallet.html` (`\n` = baris baru) |
| `FX_*`, `COINGECKO_API_KEY` | Kurs USD/IDR live dan oracle harga token; override di Admin > Settings |
| `X402_ALLOW_VOLATILE_TOKENS` | `1` hanya untuk testnet (token tanpa harga dihitung 1:1) |
| `X402_ORACLE_MAX_AGE_SECS` | Umur maksimum harga token untuk invoice x402 (default 600) |
| `ENABLE_STATIC_HLS` | Biarkan `0` di produksi |

Container perlu akses HTTPS keluar ke `open.er-api.com` dan `api.coingecko.com` untuk refresh kurs.
Panduan lengkap setup aplikasi dan kontrak x402: `docs/html/setup.html`.

## 4. Data & backup

| Volume | Isi |
|---|---|
| `pgdata` | Database PostgreSQL |
| `appdata` → `/data` | `storage/` (upload), `media/` (HLS VOD), `hls/` (sesi), `tmp/` |

- Aktifkan backup di Dokploy (Volume Backups atau `pg_dump` terjadwal via **Schedules**) ke S3/R2 terenkripsi.
- Untuk skala besar pindahkan video ke object storage (`STORAGE_BACKEND` + `S3_*`, lihat `STORAGE_MIGRATION.md`).

## 5. x402 watcher (opsional)

Tambahkan di Environment:

```
ENABLE_WATCHER=1
COMPOSE_PROFILES=watcher
X402_RPC_WSS=...
X402_CONTRACT_ADDRESS=...
```

lalu redeploy. Service `watcher` memakai image yang sama dengan `WATCHER_ENABLE=1`.

## 6. Alternatif: Application + Dokploy Postgres

1. **Create Service → Database → PostgreSQL 16**, catat *Internal Connection URL*.
2. **Create Service → Application**, Build Type: `Dockerfile`, path `./Dockerfile`.
3. Environment: isi `.env.dokploy.example` + `DATABASE_URL=<internal URL>`, `TRUST_PROXY_HEADERS=true`, `SESSION_COOKIE_SECURE=true`.
4. **Advanced → Volumes**: mount volume ke `/data`.
5. Domains: port `8080`.

## 7. Checklist keamanan (ISO/IEC 27001:2022)

- [ ] Tidak ada secret di repo; `.env` tetap di `.gitignore` / `.dockerignore` (A.8.4, A.5.17).
- [ ] Semua default (`secret`, `ChangeMe123!`, `setup-token-123`) sudah diganti (A.8.9).
- [ ] HTTPS aktif, `SESSION_COOKIE_SECURE=true` (A.8.24).
- [ ] Port Postgres tidak dipublikasikan; Adminer tidak di-deploy di produksi (A.8.20).
- [ ] Backup terenkripsi + uji restore berkala (A.8.13).
- [ ] Log (`RUST_LOG=info`) tidak memuat data pribadi/kartu; akses dashboard Dokploy dibatasi + 2FA (A.8.15, A.5.15).
- [ ] `X402_ADMIN_PRIVKEY` disimpan hanya di Dokploy env, akses terbatas (A.8.12).
- [ ] Data pribadi pengguna (email, wallet) diproses sesuai UU PDP No. 27/2022.

## 8. Troubleshooting

| Gejala | Penyebab / solusi |
|---|---|
| Build gagal `error communicating with database` | Pastikan build-arg `DATABASE_URL` kosong agar Postgres builder dipakai. |
| Build gagal versi Rust (`requires rustc 1.xx`) | Naikkan tag `rust:1.91-slim-bookworm` di `Dockerfile`. |
| Build OOM / lama | Tambah RAM/swap server, atau build di CI lalu deploy image dari registry. |
| 502 dari domain | Port domain harus `8080`, service `app`; cek healthcheck di log. |
| `extra migration ... failed` | Cek log, perbaiki SQL, atau set `APPLY_EXTRA_MIGRATIONS=0` untuk DB lama. |
| Upload besar gagal | Naikkan `MAX_UPLOAD_BYTES`; cek limit body Traefik/Cloudflare. |
