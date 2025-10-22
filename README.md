# 🎬 PPV Stream — Rust-Based Pay-Per-View Video Platform

**PPV Stream** is a secure video streaming application built with **Rust (Axum)** and **PostgreSQL**, designed for independent creators to monetize their content fairly through a **Pay-Per-View (PPV)** model.  

It features watermark-protected HLS streaming, authentication, upload management, and user dashboards.

**PPV Stream Rust** empowers anyone to build their own secure video streaming platform — like having your own version of **OnlyFans or Netflix**, but fully **open-source** and **privacy-controlled**.  

Videos are streamed (not downloaded), protected with **dynamic forensic watermarking**, and designed to help creators **monetize their work safely** without worrying about piracy.

🎥 **Demo on YouTube:**  
🔗 [https://www.youtube.com/watch?v=WOsDwBcD03A](https://www.youtube.com/watch?v=WOsDwBcD03A)

🔗 [https://www.youtube.com/watch?v=IuSjkMoYEHk](https://www.youtube.com/watch?v=IuSjkMoYEHk)

---

## 🌍 Vision

To make it possible for every creator, teacher, performer, or filmmaker to **earn money directly from their audience**, using a fair and transparent pay-per-view system that protects their creative rights.

PPV Stream Rust is **open-source**, **self-hosted**, and **built for creators who want independence** — no centralized platform, no gatekeepers, and no hidden fees.

---

## 💡 New Feature: C2C Marketplace

PPV Stream Rust makes it easy for anyone to create a **video streaming marketplace** — similar to **OnlyFans**, but **consumer-to-consumer (C2C)**.  

Users can **pay other users directly** to watch exclusive content, tutorials, music performances, religious broadcasts, short films, or personal vlogs.

This model allows:

- 💸 **Direct payments** between viewers and creators (no middleman)  
- 🧾 **Transparent transactions** for every pay-per-view event  
- 🌐 **Independent video portals** that anyone can host and brand as their own marketplace  

---

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
├── docker-compose.yml
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
│   └── 009_users_username_unique.sql
└── src
    ├── a
    ├── auth.rs
    ├── bootstrap.rs
    ├── config.rs
    ├── db.rs
    ├── email.rs
    ├── ffmpeg.rs
    ├── handlers
    │   ├── admin.rs
    │   ├── auth_admin.rs
    │   ├── auth_user.rs
    │   ├── mod.rs
    │   ├── pages.rs
    │   ├── password.rs
    │   ├── setup.rs
    │   ├── stream.rs
    │   ├── upload.rs
    │   ├── users.rs
    │   └── video.rs
    ├── hls.rs
    ├── main.rs
    ├── middleware.rs
    ├── models.rs
    ├── schema.sql
    ├── sessions.rs
    ├── token.rs
    ├── util.rs
    └── validators.rs

7 directories, 54 files
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

Open source for educational and non-commercial use.

© 2025 Kukuh Tripamungkas Wicaksono (Kukuh TW)  
All rights reserved.

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
