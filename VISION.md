# PPV Stream Rust — Open Source White Label Video Commerce Platform

> *"Fair streaming for creators, secure content for viewers, and freedom for everyone."*

---

## The Problem Worth Solving

The creator economy is broken — not because creators lack talent, but because the platforms that carry their work take too much and give back too little.

Consider what a creator faces today:

- **Centralized gatekeepers** decide who gets monetized, who gets demonetized, and who gets banned — often without warning or appeal.
- **Predatory revenue splits** where platforms keep 30–50% of every transaction, treating creators as tenants rather than owners.
- **Privacy erosion** where viewer data is harvested, sold, and used against the very audience creators built.
- **Vendor lock-in** where migrating to another platform means starting from zero — no data portability, no ownership of relationships.
- **Geographic payment barriers** where creators in Southeast Asia, Africa, or Latin America are excluded from Western-centric payment systems.
- **Piracy with no recourse** where leaking a paid video is trivially easy and platforms offer no forensic protection.
- **Zero white-labeling** where a creator's platform always shows the host platform's brand, not their own.

These are not edge cases. They are the daily reality for millions of creators, educators, performers, preachers, athletes, and filmmakers worldwide.

---

## The Opportunity

The internet has always promised disintermediation — connecting people directly, without a middleman taking the lion's share. But for video commerce, that promise remains largely unfulfilled.

**What if anyone could launch their own pay-per-view video platform in hours — fully branded, self-hosted, and owned entirely by them?**

That is the opportunity PPV Stream Rust was built to capture.

---

## What PPV Stream Rust Is

PPV Stream Rust is an **open source, white label video commerce platform** built with Rust, PostgreSQL, and FFmpeg. It gives creators, businesses, and communities the infrastructure to:

- **Sell access to video content** directly to viewers, with no intermediary
- **Brand the entire experience** as their own — their domain, their logo, their rules
- **Accept payment from anywhere** — crypto wallets or fiat through Stripe, PayPal, Midtrans, and Xendit
- **Protect content from piracy** using dynamic forensic watermarking baked into every stream
- **Deploy on any infrastructure** — a $5 VPS, a bare metal server, or a Kubernetes cluster

Think of it as the engine beneath your own version of OnlyFans, Vimeo On Demand, MasterClass, or a mosque's digital broadcast — but fully self-hosted, with no platform fee beyond your own hosting costs.

---

## Who This Is For

### Independent Creators
Musicians, filmmakers, comedians, fitness coaches, and any creative professional who wants to sell their work directly to fans without surrendering a third of their income to a platform.

### Educators and Online Course Builders
Teachers, trainers, and institutions that want to sell access to recorded lectures, tutorials, or masterclasses under their own brand.

### Religious and Community Organizations
Mosques, churches, temples, and community groups that broadcast exclusive services, events, or programs to paying members across the globe.

### Media Companies and Studios
Small studios, production houses, and digital publishers that want a private streaming infrastructure without licensing expensive enterprise software.

### Developers and SaaS Builders
Technical entrepreneurs who want to launch a multi-tenant video commerce marketplace — a platform where creators can sell to viewers, and the platform operator takes a transparent percentage.

---

## Key Features

### Content Protection
- **Dynamic Forensic Watermarking** — every video session is branded with the viewer's username and timestamp, embedded as a moving overlay by FFmpeg. If a video leaks, you know exactly who leaked it.
- **HLS Encryption** — content is segmented and streamed, never exposed as a direct downloadable file.
- **Session-Scoped Streams** — each playback generates a unique HLS session, preventing URL sharing.

### Monetization
- **Pay-Per-View** — viewers pay once to unlock a specific video permanently.
- **Crypto Payments via X402** — viewers pay directly from their Web3 wallet (MetaMask, etc.) using native coins or ERC-20 tokens (USDC, USDT, MATIC, ETH). Funds flow directly on-chain: 90% to the creator, 10% to the platform operator.
- **Fiat Payments via Plugin Architecture** — Stripe, PayPal, Midtrans (Indonesia), and Xendit (Southeast Asia) are built-in providers. Adding a new payment gateway is a matter of implementing a single Rust trait.
- **Auto-Disbursement** — with Xendit, 90% of every payment is automatically transferred to the creator's bank account without manual intervention.
- **Creator C2C Marketplace** — the platform supports a consumer-to-consumer model where users sell content to other users, making it possible to build a multi-creator marketplace from day one.

### Creator Tools
- **Self-Service Upload** — creators upload MP4 files; the platform handles transcoding, storage, and delivery.
- **Adaptive Bitrate Streaming (ABR)** — FFmpeg generates 240p, 360p, and 480p renditions automatically, so viewers on slow connections still get a smooth experience.
- **Manual Allowlist** — creators can grant free access to specific users (collaborators, reviewers, sponsors) without requiring payment.
- **Creator Profile Pages** — each creator has a public profile with their bio, bank account details for payouts, and linked blockchain wallet.
- **Per-Video Pricing** — each video carries its own price, giving creators full flexibility over their catalog.

### Administration
- **Admin Dashboard** — monitor users, sessions, videos, purchases, allowlists, and fiat invoices from a single panel.
- **SMTP Email Notifications** — configurable email delivery for password resets, change confirmations, and platform alerts.
- **Manual Disburse Control** — administrators can trigger payouts for any provider that does not support automatic disbursement.
- **Bootstrap Admin** — secure one-time setup flow for initializing the first administrator account.

### Technical Excellence (Built on Rust)
- **Memory safety without a garbage collector** — Rust's ownership model eliminates entire classes of runtime bugs that plague Node.js or Python-based alternatives.
- **Async-first architecture** — Tokio + Axum handle thousands of concurrent connections with minimal resource consumption.
- **Buffered, atomic uploads** — files are written to a `.part` temporary file and atomically renamed, preventing half-written uploads from ever reaching the database.
- **Streaming HLS delivery** — segments are streamed via `ReaderStream`, not loaded into RAM, enabling the server to handle many concurrent viewers without memory pressure.
- **HMAC-signed session cookies** — sessions are cryptographically protected; forged cookies are rejected at the middleware layer.
- **Plugin-based payment registry** — all payment providers implement a common `PaymentPlugin` trait, making the addition of new gateways a clean, isolated operation.

---

## The Smart Contract Layer — Trustless Commerce

For crypto payments, PPV Stream Rust ships with the **X402Splitter** smart contract — a Solidity contract deployed on any EVM-compatible blockchain.

When a viewer pays:
1. A unique invoice hash (Keccak256) is created and signed by the backend.
2. The viewer's wallet calls `payNative` or `payERC20` on the contract.
3. The contract validates the signature, splits the payment on-chain (e.g., 90% creator / 10% admin), and emits a `Paid` event.
4. The backend listens for the event, verifies the invoice hash against the video ID, and unlocks access.

No escrow. No trust required. The math runs on the blockchain.

---

## The White Label Promise

Every public-facing page — the marketplace, the viewer dashboard, the login screen, the video watch page — is a plain HTML + CSS + JavaScript file in the `/public` directory. There is no framework lock-in, no component library to fight against, no build step required.

To white-label the platform:
- Replace the logo.
- Update the color palette in `styles.css`.
- Change the page titles and metadata.
- Deploy under your own domain.

The result is a platform that looks and feels entirely like yours — because it is.

---

## The Architecture That Scales

```
Creator Browser          Viewer Browser
      │                        │
      │  Upload MP4            │  Pay → Watch
      ▼                        ▼
┌─────────────────────────────────────────┐
│           Rust Backend (Axum)           │
│  Auth · Upload · Transcode · Pay · HLS  │
└────────────┬────────────────┬───────────┘
             │                │
             ▼                ▼
      ┌─────────────┐  ┌──────────────┐
      │  PostgreSQL  │  │ File Storage  │
      │  (metadata, │  │ (MP4, HLS,   │
      │  sessions,  │  │  segments)   │
      │  payments)  │  └──────────────┘
      └─────────────┘
             │
             ▼
      ┌─────────────┐
      │  Blockchain  │
      │  (X402, on- │
      │  chain pay) │
      └─────────────┘
```

The stack is intentionally minimal: one Rust binary, one PostgreSQL database, one file storage volume. There are no microservices to orchestrate, no message brokers to maintain, no caches to warm. The system is easy to reason about, easy to deploy, and easy to operate.

---

## The Vision

The internet deserves a world where:

- A filmmaker in Lagos can sell access to their documentary directly to viewers in Tokyo, receiving payment in USDC with zero banking friction.
- A yoga teacher in Bali can run a subscription-free, pay-per-class studio under their own brand, keeping 90 cents of every dollar.
- A religious community in Jakarta can broadcast Friday prayers to diaspora members worldwide, funded by voluntary pay-per-view contributions.
- A developer in São Paulo can launch a multi-creator video marketplace in a weekend, building a sustainable business on top of open source infrastructure.

**PPV Stream Rust is the foundation that makes all of this possible — free to use, free to modify, and free to own.**

---

## Get Started

```bash
git clone https://github.com/kukuhtw/ppv_stream_rust
cd ppv_stream_rust
cp .env.example .env   # configure your environment
make db-up             # start PostgreSQL
make migrate           # run all migrations
make build             # compile release binary
make run               # launch the platform
```

The platform is live at `http://localhost:8080`.

---

## Built By

**Kukuh Tripamungkas Wicaksono**
Email: kukuhtw@gmail.com
GitHub: https://github.com/kukuhtw/ppv_stream_rust
LinkedIn: https://id.linkedin.com/in/kukuhtw

Licensed under the Apache 2.0 License. Use it, fork it, build on it.

---

*The creator economy needs infrastructure that works for creators, not against them. This is ours to build — together.*
