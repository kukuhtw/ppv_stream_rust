# Data Flow Guide

-> [README.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/README.md) | [ERD.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/ERD.md) | [TECHNICAL_DOCUMENTATION.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/TECHNICAL_DOCUMENTATION.md)

This document explains how data moves through the PPV Stream application from account creation to login, purchase, access entitlement, disbursement, affiliate payout, chat, and playback.

It is written as a narrative operational guide rather than a pure schema reference.
For database structure details, also read:

- [ERD.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/ERD.md)
- [PAYMENT.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/PAYMENT.md)
- [AFFILIATE.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/AFFILIATE.md)
- [WALLET.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/WALLET.md)
- [TECHNICAL_DOCUMENTATION.md](/c:/rust/github_ppv_stream_rust/ppv_stream_rust/TECHNICAL_DOCUMENTATION.md)

## Purpose

This file answers a practical question:

How does the application actually move data through its major workflows?

The key flows are:

- user registration and login
- creator profile and video publishing
- purchase via wallet, x402, or fiat gateway
- post-payment entitlement creation
- creator disbursement behavior
- affiliate commission payout
- chat persistence
- playback session generation and stream access

## System-Wide Mental Model

At a high level, the application behaves like this:

1. A user account is created and authenticated.
2. A creator uploads a video and sets a price.
3. A buyer chooses a payment rail.
4. The payment rail confirms the sale.
5. The platform writes durable business records:
   invoice, purchase, allowlist, wallet rows, affiliate rows, or a combination of them.
6. The viewer is authorized to watch.
7. A temporary playback session is created for secure HLS delivery.

Across all flows, the core business result is the same:

- the buyer gets access
- the sale is recorded
- optional affiliate logic is applied
- creator payout state is tracked according to the chosen payment method

## Core Data Objects

Before walking through the flows, it helps to know the main data objects involved:

- `users`: account identity, roles, profile, wallet balance
- `sessions`: authenticated browser sessions
- `videos`: creator-owned content
- `purchases`: durable proof of a completed sale
- `allowlist`: playback authorization records
- `x402_invoices`: blockchain payment intent and confirmation state
- `fiat_invoices`: hosted gateway payment state
- `wallet_transactions`: internal balance ledger
- `affiliate_settings`: per-video affiliate configuration
- `affiliate_commissions`: commission ledger
- `chat_conversations` and `chat_messages`: durable messaging history
- `playback_sessions`: temporary secure playback contexts

## 1. Registration and Account Creation Flow

This flow begins when a new user signs up.

### Step-by-step

1. The browser submits `POST /auth/register`.
2. The backend validates the submitted fields such as `username`, `email`, and `password`.
3. The password is hashed using Argon2.
4. A new row is inserted into `users`.
5. The user is redirected to the login flow.

### Data written

- `users`

### Important outcomes

- the user now has a stable `id`
- the user can later act as buyer, creator, affiliate, or admin if promoted
- no session is created yet unless the implementation explicitly logs the user in afterward

## 2. Login and Session Establishment Flow

This flow turns an existing account into an authenticated browser session.

### Step-by-step

1. The browser submits `POST /auth/login`.
2. The backend looks up the user by email.
3. The stored `password_hash` is verified.
4. A new `sessions` row is created with:
   user identity, admin marker, creation time, and expiry time.
5. The server sends a signed session cookie to the browser.
6. On later requests, protected endpoints read the cookie, validate the signature, load the session, and enforce expiry.

### Data written

- `sessions`

### Data read

- `users`
- `sessions`

### Important outcomes

- the browser can access protected user APIs
- the session state carries authorization context
- admin login follows the same general pattern but requires `is_admin = true`

## 3. Password Recovery and Reset Flow

This flow handles forgotten passwords.

### Step-by-step

1. The user submits `POST /auth/forgot`.
2. The backend validates the email format.
3. If the email exists, the backend creates a single-use token row in `password_resets`.
4. The backend loads SMTP settings from `smtp_settings`.
5. A reset email is sent containing the reset token.
6. The user later submits `POST /auth/reset`.
7. The backend verifies that the token exists, is not expired, and has not been used.
8. The user password is re-hashed and written to `users`.
9. The reset token row is marked used.

### Data written

- `password_resets`
- `users`

### Data read

- `users`
- `password_resets`
- `smtp_settings`

## 4. Creator Profile Setup Flow

Before using some payout and commerce features, a creator configures their profile.

### Step-by-step

1. The creator opens the profile page.
2. The frontend loads the current profile via the profile API.
3. The creator saves:
   bank payout details, wallet address, preferred wallet chain, WhatsApp, and profile description.
4. The backend validates fields such as EVM wallet format where relevant.
5. The backend updates the `users` row.

### Data written

- `users`

### Why this matters

These fields drive later flows:

- `bank_account` supports manual or Xendit-linked creator disbursement
- `wallet_account` supports x402 payout destinations
- `wallet_chain_id` helps x402 payment option selection
- `profile_desc` appears in public creator-facing surfaces

## 5. Creator Video Upload and Processing Flow

This flow creates sellable content.

### Step-by-step

1. The creator submits `POST /api/upload`.
2. The backend validates file size, extension, and MIME type.
3. The upload is written safely using a temporary partial file.
4. The file is atomically renamed into place.
5. A `videos` row is inserted with metadata such as owner, title, price, and filename.
6. Background processing prepares the media for streaming.
7. FFmpeg generates an optimized MP4 and adaptive HLS renditions.
8. Once processing succeeds, the video becomes available for purchase and playback.

### Data written

- `videos`
- filesystem assets for the uploaded media and HLS output

### Data read later from this flow

- `videos` becomes the source object for browse, watch, payment, affiliate, and playback flows

## 6. Marketplace Discovery and Watch Page Initialization

This flow starts when a viewer opens the product page for a video.

### Step-by-step

1. The browser opens `watch.html?video_id=...`.
2. The frontend loads video metadata and public creator information.
3. If the URL includes `?ref=USERNAME`, the frontend keeps that referral code in browser state.
4. The page checks whether the current viewer already has access.
5. If access does not yet exist, the page shows the payment panel.
6. The page requests available payment options from `GET /api/pay/all_options`.

### Data read

- `videos`
- `users` for public creator information
- `pay_tokens`
- `payment_settings`
- optionally `users.balance_cents` for wallet balance display

### Important outcomes

- the page knows whether the viewer can already watch
- the page knows which payment rails are currently enabled
- the referral code is preserved for later purchase requests

## 7. Entitlement Logic Before Playback

Before any stream can start, the backend checks whether the viewer is allowed to watch.

### Authorization checks typically involve

1. Is the viewer the owner of the video?
2. Is the viewer manually granted through `allowlist`?
3. Does the viewer have a completed `purchases` record?
4. For some flows, is there a confirmed invoice state that should already imply access?

### Data read

- `videos`
- `allowlist`
- `purchases`
- in some implementations, `fiat_invoices`

### Important outcome

The platform treats access as a durable business entitlement, not just a temporary payment session.

## 8. Wallet Purchase Flow

This is the simplest purchase path because everything happens inside the platform database.

### Step-by-step

1. The buyer chooses the Wallet tab on the watch page.
2. The frontend submits `POST /api/wallet/pay` with:
   `video_id` and optional `ref_code`.
3. The backend verifies:
   the buyer is logged in, the buyer is not the owner, and the buyer has not already purchased the video.
4. The backend loads:
   buyer account, creator account, video price, and buyer username.
5. The backend checks `users.balance_cents` for sufficient funds.
6. A database transaction begins.
7. The buyer balance is debited.
8. The creator balance is credited with the creator split.
9. `wallet_transactions` rows are inserted for both sides of the money movement.
10. A `purchases` row is inserted.
11. An `allowlist` row is inserted or preserved.
12. The transaction commits.
13. After the main purchase commit, affiliate logic runs as a best-effort step if a referral code exists.

### Data written

- `users.balance_cents`
- `wallet_transactions`
- `purchases`
- `allowlist`
- optionally `affiliate_commissions` and additional wallet ledger rows

### Data read

- `users`
- `videos`
- `affiliate_settings`

### Creator payout behavior

For wallet purchases:

- the creator share is credited instantly inside the internal wallet
- no external disbursement is needed

### Buyer access timing

Buyer access is immediate after transaction commit.

## 9. x402 Blockchain Purchase Flow

This path uses blockchain settlement for the sale, but still depends on backend confirmation for access.

### Phase A: Quote and invoice creation

1. The buyer chooses the x402 tab.
2. The frontend submits `POST /api/pay/x402/start`.
3. The backend loads:
   video price, creator wallet address, preferred chain, and active token configuration.
4. The backend calculates the required token amount.
5. The backend creates an `x402_invoices` row, including:
   buyer, creator, video, token info, required amount, invoice UID, and optional affiliate reference.
6. The backend signs invoice data using the admin key.
7. The signed payload is returned to the frontend.

### Phase B: On-chain payment

1. The browser wallet submits the signed payment to the smart contract.
2. The contract validates the signature, deadline, and replay conditions.
3. The contract splits the payment on-chain:
   creator share and platform share are sent immediately.
4. The contract emits a payment event.

### Phase C: Backend confirmation

1. The frontend calls `POST /api/pay/x402/confirm`.
2. The backend fetches the transaction receipt from the blockchain RPC.
3. The backend verifies:
   transaction success, event integrity, invoice match, video match, and amount sufficiency.
4. The backend marks the invoice as paid.
5. The backend inserts `purchases`.
6. The backend inserts `allowlist`.
7. The backend then attempts affiliate commission processing if `affiliate_ref` exists.

### Data written

- `x402_invoices`
- `purchases`
- `allowlist`
- optionally `affiliate_commissions`
- optionally `wallet_transactions` and `users.balance_cents` for affiliate settlement

### Data read

- `videos`
- `users`
- `pay_tokens`
- `x402_invoices`
- `affiliate_settings`

### Creator payout behavior

For x402:

- creator sale proceeds are paid directly on-chain
- the platform does not need to manually disburse the sale itself

### Important nuance

Affiliate commission for x402 is still paid through the internal wallet ledger, not on-chain.

So one sale can use two rails:

- on-chain rail for creator revenue and platform share
- internal wallet rail for affiliate commission

## 10. Fiat Gateway Purchase Flow

This path covers Stripe, PayPal, Midtrans, and Xendit.

### Phase A: Invoice creation

1. The buyer chooses a fiat provider.
2. The frontend submits `POST /api/pay/:provider/start`.
3. The backend validates the provider and the selected video.
4. The backend inserts a `fiat_invoices` row in `pending` state.
5. The row stores:
   buyer, creator, video, amount, currency, provider, optional buyer email, metadata, and optional affiliate reference.
6. The payment provider plugin creates a hosted checkout or provider-side invoice.
7. The backend stores the provider reference and payment URL.
8. The frontend redirects the buyer to the provider checkout page or opens the payment URL.

### Phase B: Provider-side payment

1. The buyer pays on the gateway side.
2. The payment provider later calls the webhook endpoint.

### Phase C: Webhook confirmation and entitlement

1. The provider hits `POST /api/pay/:provider/webhook`.
2. The backend verifies the webhook signature or trusted callback token.
3. The backend loads the matching `fiat_invoices` row.
4. The backend marks the invoice paid if the event is valid and idempotent.
5. The backend inserts `purchases`.
6. The backend inserts `allowlist`.
7. If `affiliate_ref` exists, the backend triggers affiliate commission processing.
8. Depending on the provider, creator disbursement may happen immediately, later, or automatically.

### Data written

- `fiat_invoices`
- `purchases`
- `allowlist`
- optionally `affiliate_commissions`
- optionally `wallet_transactions` and `users.balance_cents` for affiliate settlement
- optionally invoice disbursement metadata

### Data read

- `videos`
- `users`
- `fiat_invoices`
- `affiliate_settings`
- provider plugin configuration

### Creator payout behavior by provider

- Stripe, PayPal, Midtrans:
  payment confirmation and buyer access can happen before creator disbursement; payout may remain manual
- Xendit:
  the backend can attempt automatic disbursement to the creator bank account

### Important nuance

A buyer can already have access while creator payout is still pending administrative payout work on some fiat rails.

## 11. Post-Payment Entitlement Convergence

Although payment rails differ, the business result should converge to the same durable state.

### Common post-payment outputs

- the sale exists in an invoice or ledger table
- the buyer gains a `purchases` record
- the buyer gains `allowlist` access
- the viewer can request playback
- optional affiliate settlement is attempted

This convergence is one of the most important system design ideas in the repo.

## 12. Creator Disbursement Flow

Disbursement means:

how the creator receives the creator share after a successful sale.

This is not identical across payment methods.

### Wallet purchase disbursement

1. Buyer pays from internal wallet balance.
2. Creator balance is credited immediately in `users.balance_cents`.
3. Supporting ledger rows are written to `wallet_transactions`.
4. No external payout step is needed.

### x402 disbursement

1. Buyer pays on-chain.
2. Smart contract sends creator share directly to creator wallet.
3. The backend later confirms the payment and grants access.
4. No manual creator disbursement is required for the sale itself.

### Fiat disbursement for Stripe, PayPal, Midtrans

1. Buyer pays through the provider.
2. Webhook confirms payment.
3. Buyer access is granted.
4. The invoice remains the operational record of the paid sale.
5. Admin later disburses the creator share manually.
6. The backend records `disbursed_at` and `disburse_ref` when payout is completed.

### Fiat disbursement for Xendit

1. Buyer pays through Xendit.
2. Webhook confirms payment.
3. The backend can call the Xendit Disbursements API.
4. If successful, the invoice is marked with disbursement metadata.
5. If not successful, admin can retry or handle disbursement manually.

### Data written in disbursement-oriented flows

- `fiat_invoices.disbursed_at`
- `fiat_invoices.disburse_ref`
- sometimes admin audit records or admin notes in operational flows

## 13. Affiliate Payout Flow

Affiliate payout is a separate business concern from creator sale disbursement.

It always depends on:

- affiliate program being enabled for the video
- valid affiliate identity
- self-referral checks passing
- creator wallet balance being sufficient for the commission transfer

### Step-by-step

1. The buyer arrives with `?ref=affiliate_username`, or the referral is otherwise captured during checkout.
2. The referral code is stored on the invoice or passed with the wallet payment request.
3. After the purchase is confirmed, the backend calls the affiliate commission helper.
4. The helper loads `affiliate_settings` for the video.
5. The helper resolves the affiliate user by username.
6. The helper validates:
   affiliate is not the buyer and affiliate is not the creator.
7. The commission amount is calculated.
8. The creator and affiliate wallet rows are locked consistently.
9. The creator internal wallet balance is reduced by the commission amount.
10. The affiliate internal wallet balance is increased by the commission amount.
11. Two `wallet_transactions` rows are inserted:
    creator transfer-out and affiliate transfer-in.
12. One `affiliate_commissions` row is inserted.

### Data written

- `users.balance_cents`
- `wallet_transactions`
- `affiliate_commissions`

### Important nuance

Affiliate settlement is always internal-wallet-based even when the sale was:

- on-chain x402
- fiat provider checkout

That means creator payout and affiliate payout can happen on different rails.

### Failure behavior

The platform intentionally prioritizes buyer access.

So if affiliate commission fails:

- the purchase should still succeed
- the buyer should still get access
- the affiliate step is treated as best-effort or separately recoverable

## 14. Chat Flow

The chat system supports:

- user-to-admin support chat
- user-to-user direct chat

Every message is recorded in the database.

### A. Support chat flow

1. A logged-in user opens support chat.
2. The backend creates or retrieves a single support conversation for that user.
3. The conversation is stored in `chat_conversations` with `conversation_type = admin_support`.
4. When the user sends a message, a `chat_messages` row is inserted.
5. The conversation `last_message_at` is updated.
6. Admin loads support conversation summaries from the admin dashboard.
7. Admin replies; each reply becomes another `chat_messages` row.

### B. Direct user chat flow

1. A user searches for another username.
2. The frontend requests eligible direct-chat candidates.
3. The backend creates or loads a direct conversation between the two users.
4. The conversation is stored in `chat_conversations` with `conversation_type = direct`.
5. Each sent message is appended to `chat_messages`.
6. The conversation header is updated with the latest message timestamp.

### Data written

- `chat_conversations`
- `chat_messages`

### Data read

- `users`
- `chat_conversations`
- `chat_messages`

### Important outcomes

- full conversation history is durable
- support and direct messages share the same persistence pattern
- chat data can later support moderation, support operations, or analytics

## 15. Playback Request and Stream Delivery Flow

Playback starts only after the user is authorized.

### Phase A: request playback

1. The browser calls `GET /api/request_play?video_id=...`.
2. The backend authenticates the user session.
3. The backend verifies that the user can watch:
   owner, allowlist, purchase, or equivalent completed entitlement logic.
4. The backend creates a unique playback session.
5. A `playback_sessions` row is inserted with:
   session ID, user, video, temporary directory, status, and expiry.

### Phase B: prepare personalized playback

1. The backend or worker prepares a session-specific HLS working directory.
2. The system overlays moving watermark text, typically using username and timestamp.
3. The generated HLS files are tied to that playback session.

### Phase C: stream media

1. The browser requests `GET /hls/:session/:file`.
2. The backend validates the session identifier and file name.
3. The backend checks that the playback session is still valid and not expired.
4. The requested HLS playlist or segment is served with restrictive caching headers.

### Phase D: cleanup

1. Expired playback sessions are periodically identified.
2. Temporary directories are removed.
3. The related `playback_sessions` rows are updated or deleted as needed by the cleanup logic.

### Data written

- `playback_sessions`
- temporary HLS playback assets on disk

### Data read

- `videos`
- `allowlist`
- `purchases`
- `playback_sessions`

## 16. Admin Monitoring and Operational Oversight Flow

The admin area ties together the operational view of all these flows.

### Typical admin views and what they read

- user/session/video overview:
  `users`, `sessions`, `videos`, `purchases`, `allowlist`
- payment monitoring:
  `fiat_invoices`
- wallet operations:
  `wallet_transactions`, `users.balance_cents`
- affiliate overview:
  `affiliate_commissions`, `affiliate_settings`
- support inbox:
  `chat_conversations`, `chat_messages`
- SMTP configuration:
  `smtp_settings`
- payment toggles:
  `payment_settings`

### Typical admin actions and what they write

- approve deposit:
  `wallet_transactions`, `users.balance_cents`
- complete withdrawal:
  `wallet_transactions`
- reject withdrawal with refund:
  `wallet_transactions`, `users.balance_cents`
- manual disbursement:
  `fiat_invoices.disbursed_at`, `fiat_invoices.disburse_ref`
- update SMTP:
  `smtp_settings`
- update payment toggles:
  `payment_settings`

## 17. End-to-End Story Examples

### Example A: wallet purchase with affiliate

1. User registers and logs in.
2. Creator uploads a video and sets a price.
3. Creator enables affiliate settings for the video.
4. Affiliate shares a referral link.
5. Buyer opens the watch page using that referral link.
6. Buyer pays using internal wallet.
7. Buyer balance is debited.
8. Creator balance is credited.
9. Purchase and allowlist rows are inserted.
10. Affiliate commission is deducted from creator wallet and credited to affiliate wallet.
11. Buyer requests playback and receives a session-specific stream.

### Example B: x402 purchase without affiliate

1. Buyer opens the watch page.
2. Buyer requests x402 invoice creation.
3. The backend stores the invoice and returns a signed payment payload.
4. Buyer pays with MetaMask.
5. The contract sends creator and platform shares instantly on-chain.
6. Backend confirms the transaction and updates the invoice.
7. Purchase and allowlist rows are inserted.
8. Buyer requests playback and receives HLS access.

### Example C: fiat purchase with manual creator disbursement

1. Buyer opens the watch page.
2. The backend creates a Stripe, PayPal, or Midtrans invoice record.
3. Buyer pays on the provider page.
4. Webhook confirms payment.
5. Buyer access is granted immediately through purchase and allowlist creation.
6. Admin later disburses creator funds manually.
7. Invoice payout metadata is updated.

### Example D: support chat after purchase

1. Buyer completes a purchase and can watch the video.
2. Buyer opens support chat from the dashboard.
3. A support conversation is created or reused.
4. Buyer reports an issue.
5. Admin reads the stored support thread and replies.
6. Both sides continue exchanging messages, all persisted in the database.

## 18. Design Principles Visible in the Data Flow

Several architectural principles appear repeatedly across the repo.

### Convergent entitlement

Different payment rails lead to the same access state:

- purchase recorded
- allowlist granted
- playback authorized

### Idempotent confirmation

Webhook and confirmation logic is designed to tolerate retries.

This matters for:

- gateway webhooks
- x402 confirmation
- purchase insertion
- allowlist insertion
- affiliate payout logic

### Durable audit trails

The system stores operational history in durable tables:

- `wallet_transactions`
- `affiliate_commissions`
- `x402_invoices`
- `fiat_invoices`
- `chat_messages`

### Separation between sale confirmation and payout settlement

The sale becoming successful does not always mean creator funds were disbursed in the same way or at the same time.

That distinction is especially important for:

- fiat disbursement
- affiliate payout versus creator payout
- x402 creator revenue versus internal affiliate commission

## 19. Final Summary

From a data-flow perspective, PPV Stream can be understood as one central promise:

turn a viewer payment into durable content access, while recording every important business side effect.

That means the platform consistently moves from:

- identity
- to payment intent
- to payment confirmation
- to entitlement creation
- to payout or ledger settlement
- to secure playback delivery

And around that core, the platform adds:

- affiliate monetization
- wallet-based internal transfers
- admin payout operations
- durable support chat
- audit-friendly records across all major modules
