# PPV Stream Rust - Architecture & Feature Changelog

This document summarises the major improvements delivered across the platform, from the streaming pipeline to wallet operations, affiliate flows, payment plugins, and storage administration.

Back to [README.md](README.md) | Full docs in the [Documentation Index](README.md#documentation-index)

---

## Latest: Storage Admin Workflow

The storage admin area now supports a fuller operator workflow for backend migration and recovery.

**New and updated pieces:**
- `migrations/031_storage_settings.sql` - storage settings and base migration job table
- `migrations/032_storage_migration_retry_attempts.sql` - cumulative retry counts per job
- `migrations/033_storage_migration_job_items.sql` - file-level migration item history
- `migrations/034_storage_migration_resume_support.sql` - resume source tracking and skipped file counts
- `src/handlers/admin.rs` - save/test storage settings, migration start, cancel, resume, and item inspection
- `public/admin/settings.html` - storage backend form, migration jobs table, resume button, and item filters
- `STORAGE_MIGRATION.md` - full admin storage tutorial
- `STORAGE_ADMIN_MOCKUP.md` - admin UI mockup and expected operator experience

**What the admin can do now:**
1. Save desired storage settings in the database.
2. Test connectivity to S3, MinIO, R2, B2, or another S3-compatible backend.
3. Start a background migration for uploads, media, or both.
4. Cancel a running job.
5. Resume a failed or cancelled job without re-copying object keys already marked as copied.
6. Inspect file-level results and filter to failed or retried items.

---

## Affiliate System (Migration 029)

The affiliate system lets creators grow video sales through referral links. No blockchain is required - everything is settled in the internal wallet ledger.

**New files:**
- `migrations/029_affiliate.sql` - `affiliate_settings`, `affiliate_commissions`, and `affiliate_ref` columns on invoice tables
- `src/commission.rs` - standalone commission helper used by all three payment paths
- `src/handlers/affiliate.rs` - CRUD for affiliate settings, earnings, and admin view
- `public/affiliate.html` - three-tab dashboard (earnings / creator settings / referral links)

**How it works:**
1. Creator enables the affiliate program and sets commission percentage through `/api/affiliate/settings`.
2. Affiliate copies their referral link: `/public/watch.html?video_id=X&ref=USERNAME`.
3. Buyer purchases through the link.
4. Commission is automatically deducted from creator wallet revenue and credited to the affiliate wallet.
5. The flow works across wallet, X402 crypto, and fiat gateway payments.

Full details: [AFFILIATE.md](AFFILIATE.md)

---

## Internal Wallet (Migration 028)

A pure database ledger wallet - no blockchain, no third-party processor.

**New files:**
- `migrations/028_wallet.sql` - `users.balance_cents BIGINT`, `wallet_transactions` table
- `src/handlers/wallet.rs` - balance, deposit, withdraw, transfer, and pay-video endpoints
- wallet admin endpoints appended to `src/handlers/admin.rs`
- `public/wallet.html` - user wallet UI
- `public/admin/wallet.html` - admin wallet management

**Transaction types:**

| Type | Who triggers | Effect |
|------|-------------|--------|
| `deposit` | User submits | Pending until admin approves, then balance is credited |
| `withdrawal` | User submits | Balance is held immediately; admin marks paid or rejects and refunds |
| `transfer_out` / `transfer_in` | P2P transfer | Instant, atomic, both sides receive ledger rows |
| `payment` | Wallet video purchase | Buyer debited, creator credited, purchase and allowlist inserted |
| `transfer_out` / `transfer_in` | Affiliate commission | Creator debited, affiliate credited |

Full details: [WALLET.md](WALLET.md)

---

## 3-Tab Payment Panel on watch.html

The watch page consolidates all payment methods into a single unified UI instead of separate pages.

**New endpoint:** `GET /api/pay/all_options?video_id=` returns in one call:
- wallet: buyer balance and `can_afford`
- X402: available tokens from `pay_tokens`
- fiat: active providers from `PAYMENT_PLUGINS`

**UI behaviour:**
- only tabs with available options are shown
- default tab is auto-selected: wallet, then X402, then fiat
- referral notice is shown when `?ref=` is present in the URL

---

## Payment Plugin Architecture

All fiat providers share a common `PaymentPlugin` trait. Adding a new provider means implementing `create_invoice`, `confirm_payment`, and optionally `disburse_to_creator`.

```text
src/plugins/payment/
|- traits.rs
|- registry.rs
|- models.rs
\- providers/
   |- stripe.rs
   |- paypal.rs
   |- midtrans.rs
   |- xendit.rs
   \- x402.rs
```

All providers store `affiliate_ref` on their invoice row at creation time so the webhook can pay the commission correctly.

Full details: [PAYMENT_PLUGIN_ARCHITECTURE.md](PAYMENT_PLUGIN_ARCHITECTURE.md)

---

## Storage Plugin Architecture

File storage is plugin-based. The platform can operate with local disk or an S3-compatible backend depending on runtime configuration.

Related docs:
- [STORAGE_MIGRATION.md](STORAGE_MIGRATION.md)
- [STORAGE_ADMIN_MOCKUP.md](STORAGE_ADMIN_MOCKUP.md)

---

## Video Upload Pipeline

- buffered I/O with `BufWriter`
- atomic rename from temporary upload file to final path
- real-time size limit enforcement via `MAX_UPLOAD_BYTES`
- extension whitelist via `ALLOW_EXTS`
- MIME sniffing
- cleanup on database or filesystem failure

---

## Transcoding Worker

- MP4 faststart before HLS generation
- single FFmpeg process for multi-rendition ABR (240p / 360p / 480p)
- anti-upscale ladder logic based on source height
- silent-audio fallback via `anullsrc`
- semaphore-controlled concurrency

---

## HLS Streaming

- per-viewer isolated session directory
- moving watermark with username and timestamp
- `ReaderStream` delivery for segments
- `Cache-Control: no-store` on playlists and segments

---

## Session Security

- HMAC-SHA256 signed `ppv_session` cookie
- configurable TTL through `SESSION_TOKEN_TTL`
- `HttpOnly` and `SameSite=Lax`
- admin sessions validated against `is_admin`

Full details: [ADMIN_AUTHENTICATION.md](ADMIN_AUTHENTICATION.md)

---

## Configuration Highlights

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | PostgreSQL connection string |
| `HMAC_SECRET` | Signs session cookies |
| `SESSION_TOKEN_TTL` | Session lifetime in seconds |
| `ALLOW_EXTS` | Comma-separated upload extensions |
| `MAX_UPLOAD_BYTES` | Upload size limit |
| `MEDIA_DIR` | Transcoded HLS output root |
| `HLS_ROOT` | Per-viewer watermarked session root |
| `TMP_DIR` | Temporary upload directory |
| `WATERMARK_FONT` | Path to watermark font file |
| `PAYMENT_PLUGINS` | Enabled providers: `stripe,paypal,midtrans,xendit,x402` |
| `CREATOR_SPLIT_BP` | Creator share in basis points |
| `X402_CONTRACT_ADDRESS` | Deployed X402Splitter contract address |
| `X402_RPC_HTTP` | EVM HTTP RPC endpoint |
| `X402_ADMIN_PRIVKEY` | Admin signing key for invoice authorization |
| `STORAGE_BACKEND` | `local`, `s3`, `minio`, `r2`, or `b2` |
| `ADMIN_BOOTSTRAP_TOKEN` | One-time token for initial admin setup |

---

## Database Migration History

| Migration | Adds |
|-----------|------|
| 001-012 | Core: users, sessions, videos, allowlist, purchases, profile |
| 013-024 | X402: pay_tokens, x402_invoices, compatibility views |
| 025-027 | Fiat invoices, payment plugin schema, SMTP settings |
| 028 | `users.balance_cents`, `wallet_transactions` |
| 029 | `affiliate_settings`, `affiliate_commissions`, `affiliate_ref` on invoices |
| 030 | payment settings persistence |
| 031-034 | storage settings, migration jobs, retries, item history, and resume support |

---

## Related Documentation

- [README.md](README.md) - platform overview and quick start
- [SETUP.md](SETUP.md) - setup and local run guide
- [DEPLOYMENT.md](DEPLOYMENT.md) - deployment guide
- [WALLET.md](WALLET.md) - wallet system deep-dive
- [AFFILIATE.md](AFFILIATE.md) - affiliate system deep-dive
- [PAYMENT.md](PAYMENT.md) - all payment methods
- [STORAGE_MIGRATION.md](STORAGE_MIGRATION.md) - storage migration operations
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) - full codebase reference
