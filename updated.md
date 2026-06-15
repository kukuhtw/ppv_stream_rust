# PPV Stream Rust – Latest Architecture Updates

This document summarizes the major improvements introduced in the latest version of the platform, focusing on performance, security, maintainability, payment extensibility, and operational robustness.

---

# 1. Video Upload Pipeline

## Previous Implementation

* Uploaded files were written directly to the final destination.
* No atomic protection against partial uploads.
* Limited validation of uploaded content.
* Failed database operations could leave orphaned files.

## Current Implementation

* Buffered I/O using `BufWriter` to reduce disk write overhead.
* Uploads are written to a temporary `*.part` file first.
* Atomic rename is performed only after upload completes successfully.
* Real-time upload size validation using `MAX_UPLOAD_BYTES`.
* Extension whitelist validation using `ALLOW_EXTS`.
* Best-effort MIME type detection using file signature inspection.
* Automatic cleanup when database persistence fails.
* Improved operational logging.

Benefits:

* Better performance.
* Reduced filesystem corruption risk.
* Stronger upload security.
* Cleaner failure recovery.

---

# 2. Video Transcoding and HLS Generation

## Previous Implementation

* HLS generation logic was fragmented.
* Adaptive bitrate handling was inconsistent.
* Startup playback performance could be improved.

## Current Implementation

* MP4 faststart optimization using:

```text
-movflags +faststart
```

* Multi-bitrate HLS generation in a single FFmpeg process.
* Adaptive bitrate ladder:

```text
240p
360p
480p
```

* Automatic anti-upscaling.
* Audio fallback support using `anullsrc`.
* Concurrency control through `Semaphore`.
* Structured output folders:

```text
media/<video_id>/
├── master.m3u8
├── v0/
├── v1/
└── v2/
```

* Improved database processing state tracking:

```text
processing
ready
error
```

Benefits:

* Lower CPU usage.
* Faster streaming startup.
* Better operational visibility.

---

# 3. FFmpeg Runtime Layer

## New Capabilities

Reusable FFmpeg execution helper:

```rust
run_ffmpeg(args, work_dir)
```

Additional probing utilities:

```text
ffprobe_duration()
ffprobe_dimensions()
ffprobe_has_audio()
```

Advantages:

* Cleaner code separation.
* Easier maintenance.
* Safer working directory management.

---

# 4. Streaming and HLS Delivery

## Previous Implementation

* Segment files were often loaded completely into memory.

## Current Implementation

* Streaming delivery using:

```rust
ReaderStream
```

* Consistent cache control:

```text
Cache-Control: no-store
```

* Stronger path validation.
* Better HLS segment handling.
* Dynamic watermark rendering support.
* FFmpeg thread count optimized using available CPU cores.

Benefits:

* Lower memory consumption.
* Better scalability.
* Improved streaming stability.

---

# 5. Session Management and Authentication

## Previous Implementation

* Plain session identifier stored in cookie.
* Fixed expiration period.
* No cryptographic integrity protection.

## Current Implementation

* HMAC-SHA256 signed session cookies.
* Configurable session lifetime.
* HttpOnly cookie support.
* SameSite=Lax policy.
* Session validation protected against cookie tampering.

Cookie format:

```text
base64(session_id).base64(signature)
```

Benefits:

* Stronger security.
* Protection against forged session cookies.

---

# 6. Configuration and Storage Layout

## New Improvements

Dedicated storage separation:

```text
MEDIA_DIR
HLS_ROOT
TMP_DIR
```

New environment variables:

```text
ALLOW_EXTS
MAX_UPLOAD_BYTES
SESSION_TOKEN_TTL
HMAC_SECRET
MEDIA_DIR
HLS_ROOT
TMP_DIR
WATERMARK_FONT
```

Startup logging now redacts sensitive database credentials.

---

# 7. Security Hardening

Major improvements:

* Atomic file writes.
* Upload size limits.
* MIME validation.
* Signed cookies.
* Session expiration cleanup.
* Safer streaming delivery.
* Improved error tracking.
* Better filesystem isolation.

Database diagnostics now store:

```text
last_error
```

for troubleshooting and operational monitoring.

---

# 8. Payment Plugin Architecture (New)

A major architectural enhancement is the introduction of a provider-neutral payment system.

## New Structure

```text
src/plugins/payment/
├── env.rs
├── models.rs
├── traits.rs
├── registry.rs
└── providers/
    ├── x402.rs
    ├── paypal.rs
    ├── stripe.rs
    ├── midtrans.rs
    └── xendit.rs
```

## Supported Providers

```text
x402
PayPal
Stripe
Midtrans
Xendit
```

## Core Design

All payment providers implement a common trait:

```rust
PaymentPlugin
```

This allows runtime provider selection without changing business logic.

## New API Endpoints

```text
GET  /api/pay/providers
POST /api/pay/start
POST /api/pay/confirm
POST /api/pay/:provider/start
POST /api/pay/:provider/confirm
```

## Benefits

* Easier provider integration.
* Cleaner separation of concerns.
* Multi-country payment support.
* Reduced vendor lock-in.

---

# 9. CI/CD Improvements

A GitHub Actions workflow has been added.

```text
.github/workflows/rust-ci.yml
```

Validation steps:

```text
cargo fmt
cargo check
cargo clippy
cargo test
```

Benefits:

* Automated quality control.
* Faster detection of regressions.
* Better development workflow.

---

# 10. Overall Impact

The latest architecture is significantly stronger than the previous implementation.

Key improvements:

```text
✔ Faster uploads
✔ Faster streaming startup
✔ Lower memory consumption
✔ Better HLS scalability
✔ Stronger authentication security
✔ Better operational observability
✔ Modular payment architecture
✔ Easier future customization
✔ CI/CD readiness
✔ Cleaner Rust code organization
```

The platform is now positioned to evolve from a simple PPV video application into a customizable creator platform capable of supporting multiple payment providers, advanced monetization strategies, and future plugin-based extensions.