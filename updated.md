# PPV Stream Rust — Architecture & Feature Changelog

This document summarises all major improvements delivered to date, from the core streaming pipeline to the wallet system, payment panel, and affiliate program.

→ Back to [README.md](README.md) | Full docs in the [Documentation Index](README.md#-documentation-index)

---

## Latest: Affiliate System (Migration 029)

The affiliate system lets creators grow video sales through referral links. No blockchain needed — everything is settled in the internal wallet ledger.

**New files:**
- `migrations/029_affiliate.sql` — `affiliate_settings`, `affiliate_commissions`, `affiliate_ref` columns on invoice tables
- `src/commission.rs` — standalone commission helper used by all three payment paths
- `src/handlers/affiliate.rs` — CRUD for affiliate settings, earnings, admin view
- `public/affiliate.html` — three-tab dashboard (earnings / creator settings / referral links)

**How it works:**
1. Creator enables affiliate program and sets commission % (0–90) via `/api/affiliate/settings`.
2. Affiliate copies their referral link: `/public/watch.html?video_id=X&ref=USERNAME`.
3. Buyer purchases through the link — commission is automatically deducted from creator's wallet and credited to affiliate's wallet.
4. Works across all three payment methods: wallet, X402 crypto, and fiat gateway.

→ Full details: [AFFILIATE.md](AFFILIATE.md)

---

## Internal Wallet (Migration 028)

A pure database ledger wallet — no blockchain, no third-party processor.

**New files:**
- `migrations/028_wallet.sql` — `users.balance_cents BIGINT`, `wallet_transactions` table
- `src/handlers/wallet.rs` — balance, deposit, withdraw, transfer, pay-video endpoints
- Wallet admin endpoints appended to `src/handlers/admin.rs`
- `public/wallet.html` — user wallet UI
- `public/admin/wallet.html` — admin wallet management

**Transaction types:**

| Type | Who triggers | Effect |
|------|-------------|--------|
| `deposit` | User submits | Pending until admin approves, then balance credited |
| `withdrawal` | User submits | Balance held immediately; admin marks paid or rejects (refund) |
| `transfer_out` / `transfer_in` | P2P transfer | Instant, atomic, both sides get ledger rows |
| `payment` | Wallet video purchase | Buyer debited, creator credited, purchase + allowlist inserted |
| `transfer_out` / `transfer_in` | Affiliate commission | Creator debited, affiliate credited |

→ Full details: [WALLET.md](WALLET.md)

---

## 3-Tab Payment Panel on watch.html

The watch page now consolidates all payment methods into a single unified UI instead of separate pages.

**New endpoint:** `GET /api/pay/all_options?video_id=` returns in one call:
- Wallet: buyer balance, `can_afford` flag
- X402: available tokens from `pay_tokens`
- Fiat: active providers from `PAYMENT_PLUGINS` env

**UI behaviour:**
- Only tabs with available options are shown.
- Default tab is auto-selected: Wallet (if can_afford) → X402 → Fiat.
- Referral notice shown when `?ref=` is present in URL.

---

## Payment Plugin Architecture

All fiat providers share a common `PaymentPlugin` trait. Adding a new provider means implementing three methods: `create_invoice`, `confirm_payment`, and optionally `disburse_to_creator`.

```
src/plugins/payment/
├── traits.rs      ← PaymentPlugin trait
├── registry.rs    ← Runtime selection from PAYMENT_PLUGINS env
├── models.rs      ← Shared request/response types
└── providers/
    ├── stripe.rs
    ├── paypal.rs
    ├── midtrans.rs
    ├── xendit.rs
    └── x402.rs
```

All providers store `affiliate_ref` on their invoice row at creation time so the webhook can pay the commission correctly.

→ Full details: [PAYMENT_PLUGIN_ARCHITECTURE.md](PAYMENT_PLUGIN_ARCHITECTURE.md)

---

## Storage Plugin Architecture

File storage is now plugin-based. Switch between local disk and S3-compatible storage via env:

```
STORAGE_PLUGIN=local   # or s3
```

## Latest: Storage Admin Workflow

The storage admin area now supports a fuller operator workflow for backend migration and recovery.

**New and updated pieces:**
- `migrations/031_storage_settings.sql` - storage settings and base migration job table
- `migrations/032_storage_migration_retry_attempts.sql` - cumulative retry counts per job
- `migrations/033_storage_migration_job_items.sql` - file-level migration item history
- `migrations/034_storage_migration_resume_support.sql` - resume source tracking and skipped file counts
- `src/handlers/admin.rs` - save/test storage settings, migration start, cancel, resume, item inspection
- `public/admin/settings.html` - storage backend form, migration jobs table, resume button, item filters
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

## Video Upload Pipeline

- Buffered I/O (`BufWriter`) with atomic rename (`*.part` → final path).
- Real-time size limit (`MAX_UPLOAD_BYTES`), extension whitelist (`ALLOW_EXTS`), MIME sniffing.
- DB cleanup on failure; file cleaned up on DB error.

---

## Transcoding Worker

- MP4 faststart (`-movflags +faststart`) before HLS generation.
- Single FFmpeg process for multi-rendition ABR (240p / 360p / 480p).
- Anti-upscale: ladder adjusts to source height.
- Silent-audio fallback via `anullsrc`.
- Semaphore-controlled concurrency.

---

## HLS Streaming

- Per-viewer isolated session directory with moving watermark (username + timestamp).
- `ReaderStream` for segment delivery — no full-file reads into memory.
- `Cache-Control: no-store` on all playlists and segments.

---

## Session Security

- HMAC-SHA256 signed `ppv_session` cookie: `base64(sid).base64(hmac)`.
- Configurable TTL (`SESSION_TOKEN_TTL`).
- `HttpOnly` + `SameSite=Lax`.
- Admin sessions validated against `is_admin` flag.

→ Full details: [ADMIN_AUTHENTICATION.md](ADMIN_AUTHENTICATION.md)

---

## Configuration & Environment Variables

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
| `CREATOR_SPLIT_BP` | Creator's share in basis points (default 9000 = 90%) |
| `X402_CONTRACT_ADDRESS` | Deployed X402Splitter contract address |
| `X402_RPC_HTTP` | EVM HTTP RPC endpoint |
| `X402_ADMIN_PRIVKEY` | Admin signing key for invoice authorization |
| `STORAGE_PLUGIN` | `local` or `s3` |
| `ADMIN_BOOTSTRAP_TOKEN` | One-time token for initial admin setup |

---

## Database Migration History

| Migration | Adds |
|-----------|------|
| 001–012 | Core: users, sessions, videos, allowlist, purchases, profile |
| 013–024 | X402: pay_tokens, x402_invoices, compat view |
| 025–027 | Fiat invoices, payment plugin schema, SMTP settings |
| 028 | `users.balance_cents`, `wallet_transactions` |
| 029 | `affiliate_settings`, `affiliate_commissions`, `affiliate_ref` on invoices |

---

## Related Documentation

- [README.md](README.md) — platform overview and quick start
- [WALLET.md](WALLET.md) — wallet system deep-dive
- [AFFILIATE.md](AFFILIATE.md) — affiliate system deep-dive
- [PAYMENT.md](PAYMENT.md) — all payment methods
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) — full codebase reference
