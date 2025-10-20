
---

# 🎬 PPV Stream — Rust-Based Pay-Per-View Video Platform

**PPV Stream** is a secure video streaming application built with **Rust (Axum)** and **PostgreSQL**, designed for independent creators to monetize their content fairly through a **Pay-Per-View (PPV)** model.
It features watermark-protected HLS streaming, authentication, upload management, and user dashboards.

Key features include:

🔒 HLS streaming with dynamic watermarking — prevents direct MP4 downloads and embeds user-specific identifiers to trace unauthorized sharing.
👤 Authentication & user management — ensures that only paying viewers can access protected videos.
☁️ Automated upload & transcoding (FFmpeg) — converts uploaded MP4 files into secure HLS segments for streaming.
📊 Creator & admin dashboards — monitor video sales, view counts, and manage user access seamlessly.

Built on the performance and safety of Rust, PPV Stream offers a powerful foundation for professional content distribution — ideal for online educators, indie filmmakers, and digital creators who value both monetization and content protection.

🎥 Demo on YouTube: https://www.youtube.com/watch?v=WOsDwBcD03A
--- 
https://www.youtube.com/watch?v=WOsDwBcD03A

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
* 💵 **USD → IDR Conversion** for pricing ($1 = Rp 17.000)

---

## 🧱 Project Structure

```
ppv_stream/
├── src/               # Rust source code (Axum handlers, config, db, ffmpeg)
│   ├── handlers/      # Route logic (auth, upload, stream, video, admin)
│   ├── ffmpeg.rs      # Async HLS transcoding with watermark
│   ├── main.rs        # Application entry point
│   └── config.rs      # App configuration
├── sql/               # Migration files
├── public/            # Frontend pages (HTML, CSS, JS)
├── Dockerfile         # Multi-stage Docker build
├── docker-compose.yml # Local dev setup with PostgreSQL
├── Makefile           # Common dev commands
└── README.md
```

---

## ⚙️ Quick Start

```bash
# 1️⃣ Build and start database
make db-up

# 2️⃣ Build Rust app (release)
make build

# 3️⃣ Run application
make run
```

The service will start on **[http://localhost:8080](http://localhost:8080)**.

---

## 👤 Default User Accounts (for testing)

| No | Username   | Email                                           | Password (plain) |
| -- | ---------- | ----------------------------------------------- | ---------------- |
| 1  | **user01** | [user01@example.com](mailto:user01@example.com) | `Passw0rd01!`    |
| 2  | **user02** | [user02@example.com](mailto:user02@example.com) | `Passw0rd02!`    |
| 3  | **user03** | [user03@example.com](mailto:user03@example.com) | `Passw0rd03!`    |
| 4  | **user04** | [user04@example.com](mailto:user04@example.com) | `Passw0rd04!`    |
| 5  | **user05** | [user05@example.com](mailto:user05@example.com) | `Passw0rd05!`    |
| 6  | **user06** | [user06@example.com](mailto:user06@example.com) | `Passw0rd06!`    |
| 7  | **user07** | [user07@example.com](mailto:user07@example.com) | `Passw0rd07!`    |
| 8  | **user08** | [user08@example.com](mailto:user08@example.com) | `Passw0rd08!`    |
| 9  | **user09** | [user09@example.com](mailto:user09@example.com) | `Passw0rd09!`    |
| 10 | **user10** | [user10@example.com](mailto:user10@example.com) | `Passw0rd10!`    |

---

## 🗃️ Database Schema

Tables:

* `users` — user and admin accounts
* `videos` — uploaded content
* `allowlist` — manual access control
* `purchases` — pay-per-view records
* `sessions` — login sessions
* `password_resets` — recovery tokens

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

* **Backend:** Rust + Axum + SQLx
* **Database:** PostgreSQL
* **Frontend:** HTML, CSS, JavaScript
* **Media:** FFmpeg (HLS + watermarking)
* **Session:** tower-cookies

---

## 💡 License

> **Open source** for educational and non-commercial use.
> © 2025 **Kukuh Tripamungkas Wicaksono (Kukuh TW)**
> All rights reserved.

---

### 🧠 Project Metadata

```
=============================================================================
Project : PPV Stream — Secure Pay-Per-View Video Platform
Author  : Kukuh Tripamungkas Wicaksono (Kukuh TW)
Email   : kukuhtw@gmail.com
WhatsApp: https://wa.me/628129893706
LinkedIn: https://id.linkedin.com/in/kukuhtw
=============================================================================
```

### 📜 Description

**PPV Stream** is a secure **Rust-based Pay-Per-View (PPV)** video streaming platform.
It allows independent creators to upload, sell, and stream encrypted videos
with **dynamic watermarking** to prevent piracy.
Built with **Rust (Axum)**, **PostgreSQL**, and **FFmpeg (HLS transcoding)**,
it provides fast, safe, and transparent streaming.

### 🔑 Main Features

* 👤 User & Admin Authentication
* 🎥 Video Upload and Management
* 💰 Pay-Per-View Monetization
* 💧 Randomly Moving Watermark (every 5 seconds)
* 📡 HLS (HTTP Live Streaming) with FFmpeg
* 🔒 Allowlist-based Access Control
* 💵 USD → IDR Conversion ($1 = Rp17,000)
* 🖥️ Responsive Frontend (HTML + JS)

---

<p align="center">
  © 2025 <b>Kukuh Tripamungkas Wicaksono</b><br>
  📧 <a href="mailto:kukuhtw@gmail.com">kukuhtw@gmail.com</a> |
  💬 <a href="https://wa.me/628129893706">WhatsApp</a> |
  🔗 <a href="https://id.linkedin.com/in/kukuhtw">LinkedIn</a>
</p>

---
