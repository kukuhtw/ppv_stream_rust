# 🎬 PPV Stream — Rust-Based Pay-Per-View Video Platform

**PPV Stream** is a secure video streaming application built with **Rust (Axum)** and **PostgreSQL**, designed for independent creators to monetize their content fairly through a **Pay-Per-View (PPV)** model.  

It features watermark-protected HLS streaming, authentication, upload management, and user dashboards.

**PPV Stream Rust** empowers anyone to build their own secure video streaming platform — like having your own version of **OnlyFans or Netflix**, but fully **open-source** and **privacy-controlled**.  

Videos are securely streamed rather than downloaded, enabling smooth real-time playback while protecting creators’ content from unauthorized copying, redistribution, or piracy.

🎥 **Demo on YouTube:**  
🔗 [https://www.youtube.com/watch?v=WOsDwBcD03A](https://www.youtube.com/watch?v=WOsDwBcD03A)

🔗 [https://www.youtube.com/watch?v=IuSjkMoYEHk](https://www.youtube.com/watch?v=IuSjkMoYEHk)

🔗 [https://www.youtube.com/watch?v=dm8eRdstBHY](https://www.youtube.com/watch?v=dm8eRdstBHY)

---

## 🌍 Vision

To make it possible for every creator, teacher, performer, or filmmaker to **earn money directly from their audience**, using a fair and transparent pay-per-view system that protects their creative rights.

PPV Stream Rust is **open-source**, **self-hosted**, and **built for creators who want independence** — no centralized platform, no gatekeepers, and no hidden fees.

---

## 💡 New Feature: C2C Marketplace

PPV Stream Rust makes it easy for anyone to create a **video streaming marketplace** — similar to **OnlyFans**, but **consumer-to-consumer (C2C)**.

Users can **pay other users directly** to watch exclusive content, tutorials, music performances, religious broadcasts, short films, or personal vlogs.

This model allows:

* 💸 **Direct payments** between viewers and creators (no middleman)
* 🧾 **Transparent transactions** for every pay-per-view event
* 🌐 **Independent video portals** that anyone can host and brand as their own marketplace

---

## ⚙️ Built-in X402 Smart Contract Payment

The C2C system is powered by the **X402 payment contract**, a Solidity-based module integrated into PPV Stream Rust.

With **X402**, every video purchase is securely processed on the blockchain, ensuring **trust, transparency, and automation**.

Key features of the X402 integration:

* 🔐 **Decentralized Settlement** — funds are transferred directly from viewer → creator via on-chain transaction.
* ⚖️ **Auto-Split Fees** — payments are automatically divided between the **creator (e.g., 90%)** and **platform admin (e.g., 10%)**.
* 💰 **Multi-Token Support** — users can pay using **native coins (MEGA, MATIC, ETH)** or **ERC-20 tokens (USDC, USDT, etc.)**.
* 🪙 **Transparent Ledger** — all `Paid` events are logged on-chain with invoice UID, payer, creator, and amount in wei.
* 🧾 **Invoice Hashing (Keccak256)** — every invoice has a unique hash (`invoice_uid_hash`) that binds the payment to the specific video ID.

---

**Example workflow:**

1. Viewer clicks *Buy with Crypto (X402)*.
2. System creates an on-chain invoice (`invoice_uid`).
3. MetaMask opens and executes `payNative` or `payERC20`.
4. The smart contract emits a `Paid` event — funds automatically go to the creator and admin wallets.
5. Viewer instantly gains access to the video (`allowlist` updated).

---

This makes PPV Stream Rust not only a **decentralized pay-per-view platform**, but also a **ready-to-use C2C video marketplace** with **trustless crypto payments** and **full ownership control** for every creator.

Here’s the  summary updated october 26th, 2025 — clearly structured and focused on **performance**, **security**, and **data flow** differences between the old and new logic:

---

# 1) Video Upload

* **Old:** wrote files directly to the target using `File::create` + `write_all` per chunk.
* **New:**

  * Uses **Buffered I/O** with `BufWriter` (~1 MB) → fewer syscalls.
  * Writes to a temporary `*.part` file, then **atomically renames** it → prevents half-written files.
  * Enforces **file size limit** (`MAX_UPLOAD_BYTES`) and counts bytes in real time.
  * Adds **extension whitelist** (`ALLOW_EXTS`) and **MIME sniffing** via `infer`.
  * If DB insert fails, the file is **cleaned up**.
  * Logs file size and storage location.

---

# 2) Transcoding Worker

* **Old:** no `faststart`; inconsistent ABR quality; several missing functions.
* **New:**

  * Adds **MP4 faststart** (`-c copy -movflags +faststart`) before HLS → faster initial seeking.
  * Supports **multi-rendition ABR** (240p/360p/480p) in **a single ffmpeg process** using `-filter_complex` + `-var_stream_map` → better CPU/IO efficiency.
  * Includes **anti-upscale** logic (ladder adjusts to source height).
  * Handles **silent audio** with `anullsrc` + `-shortest`.
  * Uses **Semaphore** for controlled concurrency.
  * DB status is clearly tracked: `processing → ready|error`, with `last_error` and `hls_master` path stored.
  * Clean output structure under `media/<video_id>`; includes `master.m3u8` and variant subfolders.

---

# 3) FFMPEG Runner & Probing

* **Old:** `transcode_hls` ran raw command args, no dedicated working directory.
* **New:**

  * Introduces `run_ffmpeg(args, work_dir)` → all output written inside the safe target folder.
  * `transcode_hls` now truly runs inside its `session_dir`.
  * Adds helpers: `ffprobe_duration`, `ffprobe_dimensions`, `ffprobe_has_audio`.
  * HLS ABR encoding is now a utility function respecting `hwaccel` (default: CPU).

---

# 4) Streaming (Play) & HLS Serving

* **Old:** read entire HLS file into memory (`Vec<u8>`) before sending; watermark logic similar.
* **New:**

  * Streams files using **`ReaderStream`** → no full file loaded in RAM.
  * Consistent **`Cache-Control: no-store`** headers.
  * Stricter path and extension validation.
  * Moving watermark remains, and ffmpeg threads are set to `num_cpus()`.

---

# 5) Sessions & Cookies

* **Old:** stored plain `sid` cookie; fixed 7-day TTL; no integrity protection.
* **New:**

  * TTL now configurable (`SESSION_TOKEN_TTL`).
  * Cookie **signed with HMAC-SHA256** (`b64(sid).b64(sig)`) → prevents forgery.
  * Secure cookie attributes: `HttpOnly`, `SameSite=Lax`.
  * API now requires `&Config` for access to `hmac_secret` and TTL:

    * `create_session(pool, &cfg, user_id, is_admin, cookies)`
    * `destroy_session(pool, &cfg, cookies)`
    * `current_user_id(pool, &cfg, cookies)`

---

# 6) Configuration & Directories

* **Old:** `media_dir` sometimes defaulted to `hls_root`; `tmp_dir` fixed; no `allow_exts`.
* **New:**

  * Default **`media_dir = media/`**, with separate **`hls_root`** for temporary HLS sessions.
  * **`tmp_dir`** now cross-platform (uses OS temp; `/dev/shm` on Linux if available).
  * **`ensure_dirs`** creates all required directories, including `hls_root`.
  * `allow_exts` read from `ALLOW_EXTS`.
  * Startup logs redact DB credentials.

---

# 7) Security & Robustness

* **Old:** potential race conditions / partial uploads; cookies could be forged; full-file reads for streaming.
* **New:**

  * Atomic rename + size limit + MIME validation on upload.
  * HMAC-signed cookies + expired-session cleanup.
  * Streaming I/O for HLS serving.
  * `last_error` written to DB on failures for easier diagnostics.

---

# 8) Migration Impact (Changed APIs)

* `sessions::*` functions now require `&Config`.
* `Worker::new(pool, cfg, concurrency)` stores `cfg` for TTL/dirs.
* `ffmpeg::run_ffmpeg(args, work_dir)` now used by both worker and streaming layers.
* New or updated environment variables:
  `ALLOW_EXTS`, `MAX_UPLOAD_BYTES`, `SESSION_TOKEN_TTL`, `HMAC_SECRET`, `HLS_ROOT`, `MEDIA_DIR`, `TMP_DIR`, `WATERMARK_FONT`.

---

## Summary

The new version is significantly **faster** (buffered I/O, single-process multi-rendition, faststart), **more memory-efficient** (streamed HLS delivery), and far more **secure** (HMAC cookies, path validation, size & MIME checks), while offering better **observability** (DB status and error logging).


## 🚀 Key Features

* 🔐 **User & Admin Authentication** (login/register/reset password)  
* 🎥 **Video Upload** (MP4, stored securely in `/storage/`)  
* 💧 **Dynamic Watermarking** – watermark moves randomly every few seconds  
* ⚡ **HLS Transcoding via FFmpeg** – fast, segmented streaming  
* 💰 **Pay-Per-View Access** – users pay per video  
* 👥 **Allowlist System** – creators can manually grant view access  
* 📊 **Dashboard** for video management and viewer control  
* 🖥️ **Responsive Frontend** – HTML + JS in `/public`  
* 🧩 **Admin Panel** – manage users and video content  
* 💵 **USD → IDR Conversion** for pricing ($1 = Rp17,000)  

---

## 🧱 Project Structure

```
ppv_stream/
.
├── Cargo.lock
├── Cargo.toml
├── Dockerfile
├── Makefile
├── README.md
├── a
├── contracts
│   ├── Dockerfile
│   ├── contracts
│   │   └── X402Splitter.sol
│   ├── guidance_smartcontract_deployment
│   ├── hardhat.config.js
│   ├── package.json
│   └── scripts
│       ├── check_balance.js
│       ├── deploy_x402.js
│       └── estimate_gas_cost.js
├── docker-compose.yml
├── migrations
│   ├── 013_tokens.sql
│   ├── 014_x402_invoice.sql
│   ├── 015_users_wallet_chain.sql
│   ├── 016_purchases_fk_video.sql
│   ├── 017_allowlist_idx_username.sql
│   ├── 018_invoice_uid_hash.sql
│   ├── 019_x402_core.sql
│   ├── 020_x402_invoice_hash.sql
│   ├── 021_pay_tokens.sql
│   ├── 022_pay_tokens_rename_erc20.sql
│   ├── 023_x402_underpay_and_quote.sql
│   └── 024_pay_tokens_compat_view.sql
├── public
│   ├── admin
│   │   ├── dashboard.html
│   │   └── login.html
│   ├── auth
│   │   ├── forgot_password.html
│   │   ├── login.html
│   │   ├── register.html
│   │   └── reset_password.html
│   ├── browse.html
│   ├── dashboard.html
│   ├── index.html
│   ├── styles.css
│   └── watch.html
├── sql
│   ├── 001_init.sql
│   ├── 002_admins.sql
│   ├── 003_password_resets.sql
│   ├── 004_sessions.sql
│   ├── 005_allowlist.sql
│   ├── 006_indexes.sql
│   ├── 007_perf_and_fk.sql
│   ├── 008_price_cents_bigint.sql
│   ├── 009_users_username_unique.sql
│   ├── 010_videos_hls.sql
│   ├── 011_videos_description.sql
│   └── 012_user_profile.sql
├── src
│   ├── a
│   ├── auth.rs
│   ├── bin
│   │   └── seed_dummy.rs
│   ├── bootstrap.rs
│   ├── config.rs
│   ├── db.rs
│   ├── email.rs
│   ├── ffmpeg.rs
│   ├── handlers
│   │   ├── admin.rs
│   │   ├── auth_admin.rs
│   │   ├── auth_user.rs
│   │   ├── kurs.rs
│   │   ├── me.rs
│   │   ├── mod.rs
│   │   ├── pages.rs
│   │   ├── password.rs
│   │   ├── pay.rs
│   │   ├── setup.rs
│   │   ├── stream.rs
│   │   ├── upload.rs
│   │   ├── users.rs
│   │   └── video.rs
│   ├── hls.rs
│   ├── main.rs
│   ├── middleware.rs
│   ├── models.rs
│   ├── schema.sql
│   ├── services
│   │   └── x402_watcher.rs
│   ├── sessions.rs
│   ├── token.rs
│   ├── util.rs
│   ├── validators.rs
│   └── worker.rs

13 directories, 74 files
```

---

## ⚙️ Quick Start

```bash
# 1️⃣ Build and start database
make db-up
make migrate

# 2️⃣ Build Rust app (release)
make build 

# 3️⃣ Run application
make run
make seed
```

The service will start on **http://localhost:8080**

---

## 👤 Default User Accounts (for testing)

| No | Username | Email | Password |
|----|----------|-------|----------|
| 1 | user01 | user01@example.com | Passw0rd01! |
| 2 | user02 | user02@example.com | Passw0rd02! |
| 3 | user03 | user03@example.com | Passw0rd03! |
| 4 | user04 | user04@example.com | Passw0rd04! |
| 5 | user05 | user05@example.com | Passw0rd05! |
| 6 | user06 | user06@example.com | Passw0rd06! |
| 7 | user07 | user07@example.com | Passw0rd07! |
| 8 | user08 | user08@example.com | Passw0rd08! |
| 9 | user09 | user09@example.com | Passw0rd09! |
| 10 | user10 | user10@example.com | Passw0rd10! |

---

## 🗃️ Database Schema

**Tables:**

- `users` — user and admin accounts
- `videos` — uploaded content
- `allowlist` — manual access control
- `purchases` — pay-per-view records
- `sessions` — login sessions
- `password_resets` — recovery tokens

---

## 🔐 Architecture Overview

```
┌───────────────┐
│ User Browser  │
│ (HTML + JS)   │
└──────┬────────┘
       │ HTTP
       ▼
┌───────────────────────┐
│ Rust Backend (Axum)   │
│  - Auth (user/admin)  │
│  - Upload MP4         │
│  - Allowlist / Buy    │
│  - Request HLS Token  │
│  - Serve HLS Segments │
└────────┬──────────────┘
         │
         ▼
   ┌──────────────┐
   │ PostgreSQL   │
   │ (users,      │
   │  videos,     │
   │  purchases,  │
   │  allowlist)  │
   └──────────────┘
         │
         ▼
   ┌──────────────┐
   │ File Storage │
   │  - /storage/ │
   │  - /hls/     │
   └──────────────┘
```

---

## 📦 Tech Stack

- **Backend:** Rust + Axum + SQLx
- **Database:** PostgreSQL
- **Frontend:** HTML, CSS, JavaScript
- **Media:** FFmpeg (HLS + watermarking)
- **Session:** tower-cookies

---

## 💡 License

Apache 2.0 license

---

## 🧠 Project Metadata

```
=============================================================================
Project : PPV Stream — Secure Pay-Per-View Video Platform
Author  : Kukuh Tripamungkas Wicaksono (Kukuh TW)
Email   : kukuhtw@gmail.com
WhatsApp: https://wa.me/628129893706
LinkedIn: https://id.linkedin.com/in/kukuhtw
GitHub  : https://github.com/kukuhtw/ppv_stream_rust
=============================================================================
```

### 📜 Description

PPV Stream is a secure Rust-based Pay-Per-View (PPV) video streaming platform. It allows independent creators to upload, sell, and stream encrypted videos with dynamic watermarking to prevent piracy. Built with Rust (Axum), PostgreSQL, and FFmpeg (HLS transcoding), it provides fast, safe, and transparent streaming.

### ✨ Tagline

**"Fair streaming for creators, secure content for viewers, and freedom for everyone."**

---

<p align="center">
  © 2025 <b>Kukuh Tripamungkas Wicaksono</b><br>
  📧 <a href="mailto:kukuhtw@gmail.com">kukuhtw@gmail.com</a> | 
  💬 <a href="https://wa.me/628129893706">WhatsApp</a> | 
  🔗 <a href="https://id.linkedin.com/in/kukuhtw">LinkedIn</a> | 
  💻 <a href="https://github.com/kukuhtw/ppv_stream_rust">GitHub</a>
</p>
