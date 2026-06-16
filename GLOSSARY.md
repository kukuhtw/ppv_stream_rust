# Glossary

This glossary explains the main business, payment, streaming, security, infrastructure, and code-level terms used throughout the PPV Stream Rust repository.

It is written for:

- developers onboarding to the codebase
- product owners and operators
- QA and support teams
- technical writers and implementers

The goal is to make the vocabulary of the system precise and consistent.

---

## How to Read This Glossary

Many terms in this repository are related but not identical. In particular, these are easy to confuse:

- **payment**: the buyer successfully sends money
- **purchase**: the application records that the buyer bought access to a specific video
- **access grant**: the system unlocks the video for the buyer
- **disbursement**: the creator receives their share of the sale proceeds
- **affiliate settlement**: the affiliate receives their commission when a referral is involved
- **wallet balance**: off-chain internal platform balance, not an external bank or blockchain balance

If two terms sound similar, this glossary separates them intentionally.

---

## Business Terms

### Admin

The platform operator account with elevated privileges. Admins can review payments, process manual payout workflows, manage SMTP settings, inspect wallet activity, view affiliate commission ledgers, and manage platform settings.

In this repository, admin authentication is separate from standard user authentication even though both are stored in the `users` table.

### Affiliate

A user who promotes a creator's video using a referral link and earns a commission when another user buys through that link.

An affiliate does not own the video being promoted. The affiliate's earnings come from the creator's share, not from the buyer paying more and not from the platform fee.

### Affiliate Commission

The amount paid to the affiliate when a referred buyer completes a purchase. In this platform, affiliate commission is computed as:

`purchase_price_cents × commission_pct / 100`

The commission is settled through the internal platform wallet ledger, even when the buyer paid through blockchain or a fiat payment gateway.

### Affiliate Settlement

The internal ledger movement that transfers commission value from the creator's wallet balance to the affiliate's wallet balance.

This is different from creator disbursement. A creator may receive sale proceeds on-chain or through a bank payout, while the affiliate still gets credited inside the internal wallet system.

### Affiliate Program

The per-video configuration that allows a creator to define:

- whether referral commissions are enabled
- what commission percentage applies

It is stored in `affiliate_settings`.

### Allowlist

The persistent authorization list that determines whether a user can watch a specific video.

After a successful purchase, a row is inserted into `allowlist (video_id, username)`. Once present, the buyer keeps access unless the row is explicitly removed.

### Buyer

The user who purchases access to a video. In some documents this role is also called:

- viewer
- customer
- purchaser

The buyer is the account that ultimately receives access in `allowlist`.

### Catalog

The public browseable list of videos available on the platform, shown through marketplace-style pages such as `browse.html`.

### Commission Percentage

The per-video percentage chosen by the creator for affiliate payouts. This is not the same as the platform fee and it is not the same as the creator split.

Example:

- platform fee: 10%
- creator split before affiliate: 90%
- affiliate commission: 15% of sale price

The affiliate commission reduces the creator's net result, not the platform fee.

### Creator

The user who owns a video listing and receives the creator share of revenue when the video is sold.

Creators may also configure:

- video price
- affiliate commission percentage
- bank account for fiat disbursement
- EVM wallet for x402 payout

### Creator Share

The portion of the sale assigned to the creator before affiliate commission is deducted.

It is controlled by `CREATOR_SPLIT_BP`, expressed in basis points.

Example:

- `CREATOR_SPLIT_BP=9000`
- creator share = 90%
- platform share = 10%

### Disbursement

The step where the creator receives their sale proceeds.

Disbursement behavior depends on payment method:

- wallet: creator is credited internally immediately
- x402: creator is paid directly on-chain
- Stripe / PayPal / Midtrans: creator payout is manual in the current implementation
- Xendit: creator payout can be auto-disbursed to a bank account

### Manual Disburse

A payout workflow where the admin must manually complete or confirm creator payout after the sale has already been marked paid.

This is used for fiat payment methods without an implemented auto-payout path in the platform.

### Auto-Disburse

A payout workflow where the system automatically sends the creator share after payment confirmation, without waiting for a human operator to trigger the payout.

Examples:

- x402 smart contract split
- Xendit Disbursements API

### Platform Fee

The portion of a sale retained by the platform after the creator split is calculated.

In the default configuration:

- creator share = 90%
- platform fee = 10%

This fee is separate from affiliate commission.

### Purchase

The application-level record that a buyer bought access to a specific video. A purchase is stored in the `purchases` table.

Payment alone is not enough. The system still needs to:

- record the purchase
- insert the allowlist row

to make the sale useful to the buyer.

### Referral Link

A URL that includes `?ref=USERNAME`, allowing the platform to attribute a sale to an affiliate.

Example:

`/public/watch.html?video_id=VIDEO_ID&ref=alice`

### Seller

A business synonym for creator. In this codebase, "creator" is usually the preferred term, but "seller" may appear in payment or operational explanations.

### Viewer

A user watching content on the platform. A viewer may or may not be a buyer. A viewer becomes a buyer once a paid transaction is completed and access is granted.

### White Label

A deployment model where the platform can be rebranded and operated under another company's or creator network's brand.

This repository supports white-label behavior through custom branding and self-hosted infrastructure.

---

## Payment and Commerce Terms

### Balance

The numeric amount stored in `users.balance_cents`. This is the internal platform wallet balance, not a bank balance and not a blockchain wallet balance.

### Basis Points (BP)

A finance notation where:

- 100 bp = 1%
- 10,000 bp = 100%

This repository uses basis points to represent the creator revenue split because it avoids floating-point ambiguity.

### Checkout

The user-facing payment step where the buyer selects a payment method and begins the payment flow.

Checkout may happen:

- internally in the wallet flow
- via MetaMask and smart contract interaction
- via redirect to an external payment provider

### Confirm Payment

The step where the backend decides whether the payment is truly successful.

This can happen through:

- a direct internal DB transaction in the wallet path
- on-chain transaction receipt verification in the x402 path
- webhook verification in the fiat gateway path

### Creator Net Revenue

The amount the creator effectively keeps after both platform fee and any affiliate commission.

Example:

- sale price: $10
- creator share before affiliate: $9
- affiliate commission: $1
- creator net: $8

### Fiat

Government-issued currency such as USD or IDR. In this repository, fiat payment flows are handled by payment plugins like Stripe, PayPal, Midtrans, and Xendit.

### Fiat Invoice

The DB record stored in `fiat_invoices` for a payment plugin transaction. It tracks:

- invoice UID
- provider
- payment URL
- payment status
- payout / disbursement status
- affiliate reference

### Gateway

An external payment processor or checkout provider, such as Stripe, PayPal, Midtrans, or Xendit.

### Internal Wallet

The off-chain platform-managed ledger where users can hold value, deposit, withdraw, transfer to other users, and pay for videos.

This is not a blockchain wallet. It is a database-backed accounting system.

### Ledger

An accounting-style record of balance-affecting events. In this repository, wallet activity is appended to `wallet_transactions`.

The ledger provides an audit trail for:

- deposits
- withdrawals
- transfers
- video purchases
- affiliate commission movements

### Payment Method

The mechanism the buyer uses to pay. The platform supports three major categories:

- internal wallet
- x402 blockchain payments
- fiat payment plugins

### Payment Provider

An external payment integration module such as Stripe, PayPal, Midtrans, Xendit, or x402 in the plugin architecture.

In conversation, "payment provider" and "payment plugin" are often used together, but the plugin is the code abstraction while the provider is the real-world payment rail.

### Payment Status

The normalized state of a transaction or invoice, such as:

- pending
- paid
- failed
- expired
- cancelled
- underpaid

These statuses allow the rest of the application to behave consistently across multiple providers.

### Platform Wallet

An informal term for the internal wallet system of the platform. It refers to the database-based balance model, not a blockchain wallet.

### Price Cents

The sale price stored as an integer number of smallest fiat units, usually USD cents, to avoid floating-point rounding issues.

### Purchase Price

The listed sale amount paid by the buyer for a specific video, before affiliate commission is deducted from the creator's proceeds.

### Provider Webhook

A server-to-server callback sent by a payment provider to notify the platform that a payment event occurred. Webhooks are essential for fiat payments because buyer browser redirects are not trusted as final proof of payment.

### Revenue Split

The division of each sale between:

- creator share
- platform share

Affiliate commission is applied after this split and is paid from the creator side in the current business model.

### Underpaid

A payment state where the amount received is lower than the minimum required amount. This is especially important in blockchain flows where token-denominated value can be checked against a required on-chain amount.

### Wallet Deposit

A user action that requests additional value to be added to the internal wallet. Deposits are not automatically trusted. They typically require admin review or confirmation depending on the integration path.

### Wallet Withdrawal

A user action that requests payout from the internal wallet to an external destination such as a bank account or crypto address. Withdrawal requests are tracked and generally require admin handling.

### Wallet Transfer

A direct transfer between two users inside the internal platform ledger. No external provider is involved.

### Wallet Transaction

An individual ledger row in `wallet_transactions` that represents a balance-affecting event. Common transaction types include:

- deposit
- withdrawal
- transfer_in
- transfer_out
- purchase-related entries

### Webhook Verification

The process of validating that an inbound webhook genuinely came from the payment provider and has not been tampered with.

Each provider uses a different scheme:

- Stripe: HMAC signature
- PayPal: API-based signature verification
- Midtrans: hashed concatenated fields
- Xendit: callback token header

---

## Affiliate Terms

### Affiliate Earnings

The sum of commissions earned by a specific affiliate across all referred purchases.

### Affiliate Ref

The username captured from a referral link and stored in invoice or payment context so the referral survives redirects, async confirmation, and later settlement steps.

### Affiliate Ledger

The combination of:

- `affiliate_commissions` as the audit table
- `wallet_transactions` as the money movement record

This is how the platform proves both attribution and settlement.

### Creator-Funded Commission

The rule that affiliate commission is deducted from the creator's share, not added on top of the buyer's price and not taken from the platform fee.

### Self-Referral

A disallowed case where the buyer tries to refer themselves, or the creator tries to receive affiliate commission on their own video.

### Referral Attribution

The act of linking a completed purchase to a specific affiliate identity using `ref_code` or `affiliate_ref`.

### Referral Capture

The earlier step where the system reads the referral identifier, usually from the watch page URL query parameter, and carries it into invoice creation or payment initiation.

---

## Blockchain and x402 Terms

### x402

The repository's name for the EVM-based smart-contract payment flow. In practical terms, it means:

- the buyer pays with a supported token or native coin
- the backend signs an authorization payload
- the smart contract validates and splits payment atomically

### Admin Wallet

The EVM address configured through `X402_ADMIN_WALLET`. It receives the platform share of x402 sales.

### Chain ID

The numeric identifier of an EVM network, such as:

- 80002 for Polygon Amoy
- 137 for Polygon mainnet

### Creator Wallet

The creator's EVM address stored in their profile and used as the payout destination for x402 purchases.

### EIP-191 / Signed Message Hash

The Ethereum signed-message scheme used to convert a hash into a wallet-signable payload, preventing arbitrary signature misuse.

### EVM

Ethereum Virtual Machine. The blockchain execution environment used by chains compatible with Ethereum tooling.

### Invoice UID

The application-generated unique identifier for a payment attempt. In x402, the UID is also hashed to bytes32 for event matching and replay protection.

### Native Token

The built-in coin of an EVM chain, such as MATIC on Polygon. Native token payments differ from ERC-20 token payments because they do not need a token contract address.

### On-Chain

An action that occurs on the blockchain itself, recorded in a transaction and verifiable through RPC endpoints and transaction receipts.

### Pay Tokens

The supported token and chain combinations stored in `pay_tokens`. These determine which blockchain payment options the frontend can offer.

### Receipt Verification

The backend process of fetching a transaction receipt from the blockchain RPC and verifying that the expected event was emitted by the expected contract with the expected invoice metadata.

### RPC

Remote Procedure Call endpoint used by the backend to talk to a blockchain node.

Common forms:

- HTTP RPC for querying receipts
- WebSocket RPC for event watching

### Smart Contract Split

The contract-level logic that atomically routes the creator share and platform share to separate addresses in the same blockchain transaction.

### Token Decimals

The number of fractional units used by a token. This matters when converting a fiat-denominated price into on-chain base units.

### Transaction Hash

The unique blockchain identifier of a submitted transaction. It is used later to verify whether the payment truly happened.

### Watcher

A background process that listens to blockchain events, usually via WebSocket RPC. In this repository, an x402 watcher can be enabled for monitoring or operational support, but purchase confirmation still has an explicit backend verification path.

---

## Streaming and Content Protection Terms

### Adaptive HLS

HTTP Live Streaming with multiple renditions, allowing the player to adapt video quality to bandwidth and device conditions.

### FFmpeg

The media-processing tool used for transcoding uploads, producing streaming outputs, and rendering watermark overlays.

### Forensic Watermark

A visible per-viewer watermark that helps trace leaks or unauthorized redistribution. In this repository, the watermark is generated per playback session.

### HLS

HTTP Live Streaming, the segmented video delivery protocol used by the platform.

### HLS Master Playlist

The top-level `.m3u8` file that references one or more HLS renditions.

### HLS Session

A temporary per-viewer playback session used to isolate access and support session-scoped watermarking and authorization.

### Media Directory

The filesystem location where processed media and streaming artifacts are stored or served from.

### Playback Authorization

The backend decision that determines whether a user is allowed to request streaming assets for a video.

### Rendition

A single encoded version of a video at a particular resolution or bitrate.

### Session-Scoped Streaming

The pattern where each playback request is tied to a session context rather than exposing globally shared streaming asset URLs.

### Transcoding

The conversion of an uploaded source video into optimized playback formats such as HLS renditions and fast-start MP4 files.

---

## Security and Authentication Terms

### Argon2

The password hashing algorithm used for user credentials. It is designed to be expensive to brute-force and safer than plain hash functions for password storage.

### Authentication

The process of proving user identity, such as login via email and password.

### Authorization

The process of determining what an authenticated user is allowed to do, such as viewing admin pages or watching a purchased video.

### HMAC

Hash-based Message Authentication Code. In this repository it is used for signed cookies and related integrity-sensitive flows.

### HMAC Secret

The server secret used to sign and validate session-related data. If changed, existing signed sessions become invalid.

### Session

The authenticated state linking a browser to a logged-in user. Sessions are stored in the database and tied to signed cookies.

### Signed Cookie

A browser cookie that includes an integrity check so the server can detect tampering.

### SMTP

Simple Mail Transfer Protocol. It is used for sending operational emails such as password reset messages.

### Password Reset Token

A single-use, time-limited token allowing a user to set a new password after requesting password recovery.

---

## Architecture and Code Terms

### Axum

The Rust web framework used to define routes, handlers, request extraction, and application state.

### Handler

A function that receives an HTTP request and returns a response. Most user-facing behavior in this repository is implemented as Axum handlers under `src/handlers/`.

### State

The structured application context passed into handlers, typically containing shared objects such as:

- database pool
- configuration
- registries
- worker references

### Router

The route-definition object in Axum that maps URLs and HTTP methods to handlers.

### Plugin Architecture

The design pattern used to abstract payment providers and storage backends behind shared traits and registries, allowing multiple implementations to fit the same application flow.

### Payment Plugin

A code module implementing the provider-neutral payment trait. It is the software abstraction that lets the platform integrate multiple payment rails consistently.

### Payment Plugin Registry

The runtime registry that loads and exposes enabled payment providers. It decides which providers are available to the application at runtime.

### Storage Plugin

A code module that implements file/object storage behavior. This allows the platform to switch between local disk and S3-compatible object stores.

### PgPool

The PostgreSQL connection pool shared across handlers and services. It allows multiple concurrent DB operations without opening a new connection for every query.

### SQLx

The Rust database library used in this repository. It supports:

- runtime queries
- compile-time checked query macros
- PostgreSQL integration

### Worker

The background processing component that handles queued media tasks such as video transcoding.

### Best-Effort

An implementation policy meaning the system tries to do an action, but a failure in that secondary action should not invalidate the primary business success.

Example:

- a buyer's purchase succeeds
- affiliate commission processing fails
- buyer still gets access

### Idempotent

A property meaning that running the same operation multiple times produces the same final result without duplicating side effects.

This matters for:

- webhook retries
- purchase recording
- allowlist grants
- disbursement state marking

### Runtime Query

A SQL query executed without compile-time schema validation by SQLx macros. Runtime queries are often used when schema drift or build-time DB access would otherwise be inconvenient.

### Compile-Time Query

A SQLx macro-based query checked against a database schema during build time. This improves safety but requires a working schema reference at build time.

---

## Database and Data Model Terms

### `affiliate_commissions`

The audit table that records each commission payout event, including affiliate identity, buyer identity, owner identity, payment method, and related invoice reference.

### `affiliate_settings`

The per-video affiliate configuration table storing whether the program is enabled and what commission percentage applies.

### `allowlist`

The access-grant table that determines whether a username may watch a specific video.

### `fiat_invoices`

The payment-tracking table for Stripe, PayPal, Midtrans, and Xendit transactions.

### `pay_tokens`

The configuration table describing supported blockchain payment tokens and chains.

### `purchases`

The sales table that records which user bought which video.

### `sessions`

The authentication session table mapping login state to users and expiration times.

### `smtp_settings`

The table storing configurable email delivery settings used by the application.

### `users`

The primary identity table for buyers, creators, affiliates, and admins. It also stores internal wallet balance and creator payout-related profile fields.

### `videos`

The main catalog table for uploaded content, storing metadata such as owner, title, price, and processing status.

### `wallet_transactions`

The immutable-style ledger table used to record wallet-related balance changes and admin-reviewed payout operations.

### `x402_invoices`

The blockchain payment-tracking table storing x402 payment attempts, required token amounts, invoice hashes, affiliate reference, and on-chain confirmation data.

---

## Operations and Deployment Terms

### Adminer

The lightweight database administration UI included in the Docker workflow for inspecting PostgreSQL state manually.

### Docker Compose

The multi-container orchestration setup used in local development for services such as:

- application
- PostgreSQL
- Adminer
- x402 deployer
- optional watcher

### Environment Variable

An externalized configuration value passed into the application process. This repository uses environment variables heavily for:

- database connection
- payment provider credentials
- storage backend configuration
- blockchain RPC and contract settings

### Health Check

An endpoint or container-level probe used to determine whether a service is alive and ready to receive traffic.

### Local Storage Backend

The simplest storage mode, where uploaded and processed files are stored on local disk instead of object storage.

### S3-Compatible Storage

An object-storage interface compatible with the Amazon S3 API. The repository can be adapted to services such as MinIO, AWS S3, Cloudflare R2, and similar systems through the storage plugin layer.

### Seeder

A command or script that inserts initial or demo data into the database for testing and local setup.

### Setup Admin

The bootstrap flow used to create or reset the initial admin account through a protected route and token.

---

## UI and Product Terms

### Dashboard

The creator/user-facing page where a logged-in user manages content, profile information, and related platform functions.

### Admin Dashboard

The admin-facing control surface for operations, payment review, support visibility, and system settings.

### Payment Panel

The buyer-facing UI element on the watch page that displays all available checkout methods for a video.

### Support Chat

The browser-based chat feature allowing users to contact admins and users to talk to each other, with all messages stored in the database.

### Tutorial Mockup

A documentation-oriented HTML artifact under `tutorial/mockups/` used to explain intended interface behavior without requiring the full app to run.

---

## Practical Distinctions That Matter

### Payment vs Purchase vs Access

- **payment**: money was successfully sent
- **purchase**: the app recorded the sale
- **access**: the user can now watch the video

These normally happen together, but they are distinct steps in the implementation.

### Creator Payout vs Affiliate Payout

- **creator payout**: how the seller gets the creator share
- **affiliate payout**: how the referrer gets commission

These may happen through different rails in the same sale.

Example:

- buyer pays via x402 on-chain
- creator is paid on-chain instantly
- affiliate is credited later inside the internal wallet ledger

### Auto-Disburse vs Immediate Ledger Credit

- **auto-disburse** usually means the platform sends money out automatically to an external destination
- **immediate ledger credit** means the internal platform balance is updated without leaving the platform

Wallet purchases use immediate ledger credit. Xendit and x402 can perform true external or chain-level auto-disbursement.

### Platform Wallet vs Blockchain Wallet

- **platform wallet**: internal DB-backed balance
- **blockchain wallet**: external EVM address holding native or token assets

Confusing these two leads to incorrect payout assumptions, especially in affiliate explanations.

---

## Related Documents

- [README.md](README.md)
- [SETUP.md](SETUP.md)
- [PAYMENT.md](PAYMENT.md)
- [AFFILIATE.md](AFFILIATE.md)
- [WALLET.md](WALLET.md)
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md)
