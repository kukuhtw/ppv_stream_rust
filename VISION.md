# PPV Stream Rust — Open Source White Label Video Commerce Platform

> *"Fair streaming for creators, secure content for viewers, and freedom for everyone."*

→ [README.md](README.md) | [WALLET.md](WALLET.md) | [AFFILIATE.md](AFFILIATE.md) | [PAYMENT.md](PAYMENT.md)

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

### Digital Agencies and Brand Service Teams
Digital agencies can use PPV Stream Rust as more than a creator tool. They can use it as a white-label premium media and video-commerce system for client brands.

This creates a strong positive opportunity for agencies:

- they can move beyond campaign execution into owned-platform infrastructure
- they can offer recurring retainers around branded premium-content operations
- they can help clients capture first-party audience relationships instead of sending traffic back to third-party platforms
- they can combine strategy, design, content production, monetization, payment setup, and gated access into one higher-value service

For client brands, that means an agency can help launch:

- premium video libraries
- paid webinar and training portals
- gated product-launch content
- exclusive behind-the-scenes campaigns
- ambassador and affiliate-led conversion funnels
- private membership or loyalty video hubs
- regional paid content experiences using local payment methods

This is especially useful for agencies serving:

- consumer brands
- education brands
- media brands
- influencers and personal brands
- religious or community organizations
- event organizers

Instead of delivering only traffic and content assets, an agency can deliver a branded monetization system under the client's own domain, with stronger control over payments, access rules, affiliate campaigns, content protection, and customer experience.

### Brands Building Video Creator Communities
Brands can use PPV Stream Rust not only as a premium video sales channel, but also as a structured environment for building a long-term creator community around their products, values, culture, and audience.

Many brands already work with creators, ambassadors, resellers, educators, fans, or community leaders. The problem is that those relationships are often scattered across public social platforms where the brand does not control access, monetization, audience data, or continuity. A white-label video commerce platform gives the brand a more intentional home for that ecosystem.

With PPV Stream Rust, a brand can:

- invite selected creators into a branded video marketplace or content hub
- allow multiple creators to publish under one brand-owned environment
- reward creators through direct content sales, affiliate revenue, referrals, or special access programs
- organize campaigns around launches, tutorials, behind-the-scenes stories, challenges, or community education
- preserve first-party audience relationships instead of relying only on algorithm-driven reach

This helps a brand evolve from simply sponsoring creators into cultivating a creator economy inside its own ecosystem.

This is especially useful for brands that want to:

- nurture ambassadors over time instead of running one-off campaigns
- turn customers into storytellers, educators, reviewers, or advocates
- create premium storytelling programs tied to product categories or shared identity
- give niche creators a place to monetize deeper expertise that would be buried on mass platforms
- build community loyalty through recurring creator-led content rather than only paid ads

In practice, a brand could use the platform to host:

- creator-led product education series
- community testimonials and transformation stories
- paid expert sessions or workshops
- exclusive campaign documentaries
- regional language content from local creators
- member-only video challenges or community programs
- affiliate-led conversion funnels where creators are rewarded when their stories drive purchases

The result is not just branded media. It is a branded creator network with economic incentives, where the platform becomes part of the brand's long-term relationship infrastructure.

### Event Organizers and Experiential Marketing Teams
Event organizers can also use PPV Stream Rust as a valuable post-production, premium-access, and brand-partner distribution layer for their events.

This is a positive opportunity because an event organizer does not need to stop at ticket sales or one-day attendance. They can extend the commercial life of an event by turning recorded sessions, backstage content, VIP recaps, workshop replays, partner activations, and sponsor-exclusive media into a controlled branded video experience.

For event clients, brand partners, and sponsors, an organizer can use the platform to:

- deliver paid or invite-only event replays
- create sponsor-branded content hubs after the event
- provide premium workshop archives for attendees who missed parallel sessions
- sell backstage interviews, keynote replays, or extended-cut footage
- give brand partners gated access to campaign recap videos and performance storytelling assets
- run affiliate or ambassador referral campaigns tied to event content sales
- keep long-tail event monetization alive long after the physical event has ended

This is especially useful for:

- conferences
- summits
- expos
- concerts
- community festivals
- training events
- hybrid and virtual events

That means event organizers can serve client brands and partners with more than stage production and event logistics. They can also offer a branded digital content layer that extends audience reach, protects premium recordings, supports sponsor value delivery, and opens new revenue after the event is over.

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

## What We've Built

Beyond the streaming core, PPV Stream Rust now includes a complete commerce layer:

### Internal Wallet
Every user has a wallet balance on the platform — a pure database ledger, no blockchain. Creators receive their revenue share directly into their balance. Users can top up via admin-approved deposits, withdraw via admin-processed payouts, and transfer between each other instantly. The wallet is also the payment method for purchasing videos — no external service needed.

→ [How the wallet works →](WALLET.md)

### Affiliate System
Creators who want to grow their audience can enable an affiliate program on any video. They set a commission percentage (up to 90% of the video price). Affiliates share a unique referral link (`?ref=USERNAME`). When a buyer purchases through that link, the affiliate earns their commission automatically — deducted from the creator's wallet balance and credited to the affiliate's balance. This works across all three payment methods: wallet, crypto, and fiat.

→ [How the affiliate system works →](AFFILIATE.md)

### Three-Path Payment Panel
Buyers see a unified payment panel with three tabs: Wallet (instant, no crypto needed), X402 Crypto (MetaMask), and Payment Gateway (Stripe/PayPal/Midtrans/Xendit). The system auto-selects the best available option based on what the admin has configured and whether the buyer can afford it from their wallet.

→ [All payment methods →](PAYMENT.md)

---

## The Vision

The internet deserves a world where:

- A filmmaker in Lagos can sell access to their documentary directly to viewers in Tokyo, receiving payment in USDC with zero banking friction.
- A yoga teacher in Bali can run a subscription-free, pay-per-class studio under their own brand, keeping 90 cents of every dollar.
- A religious community in Jakarta can broadcast Friday prayers to diaspora members worldwide, funded by voluntary pay-per-view contributions.
- A developer in São Paulo can launch a multi-creator video marketplace in a weekend, building a sustainable business on top of open source infrastructure.
- A brand can gather customers, ambassadors, and independent creators into one shared video ecosystem where authentic stories strengthen community and successful creators earn from the value they create.

It also deserves a world where storytelling itself becomes economically meaningful for ordinary users, not only celebrities or large media companies.

PPV Stream Rust helps make that possible by giving users a place to:

- tell real stories through premium video, education, testimony, commentary, or community experience
- publish for an audience that chooses to pay for relevance, trust, or access
- earn revenue from fellow users in a direct C2C model
- participate in affiliate, referral, or brand-community programs without losing ownership of their identity
- build reputation and recurring income from authentic contribution, not only viral reach

In that model, the platform is not merely a video locker or checkout page. It becomes a place where people can narrate what they know, what they have lived, what they believe in, or what they are building - and be rewarded by other users who find that story valuable.

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

---

## Documentation

| Document | What it covers |
|----------|---------------|
| [README.md](README.md) | Quick start, feature list, architecture |
| [WALLET.md](WALLET.md) | Internal wallet — deposits, withdrawals, transfers, video purchases |
| [AFFILIATE.md](AFFILIATE.md) | Affiliate referral program — setup, commission flows, earnings |
| [PAYMENT.md](PAYMENT.md) | All payment methods — wallet, X402, Stripe, PayPal, Midtrans, Xendit |
| [PAYMENT_PLUGIN_ARCHITECTURE.md](PAYMENT_PLUGIN_ARCHITECTURE.md) | How payment providers are structured |
| [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) | Full codebase reference |
| [updated.md](updated.md) | Changelog — latest architecture improvements |
