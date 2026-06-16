# Payment System — How It Works

→ [README.md](README.md) | [WALLET.md](WALLET.md) | [AFFILIATE.md](AFFILIATE.md) | [PAYMENT_PLUGIN_ARCHITECTURE.md](PAYMENT_PLUGIN_ARCHITECTURE.md)

## Overview

This platform is a **Pay-Per-View (PPV) streaming service**. Buyers pay directly to access a specific video. Revenue is automatically split between the **creator (owner)** and the **platform admin** at the moment of payment.

**Three payment paths** are available, all shown in a unified tab panel on `watch.html`:

1. **Internal Wallet** — instant, no crypto wallet needed; balance funded via deposit.
2. **X402 (on-chain crypto)** — MetaMask; funds split on-chain via smart contract.
3. **Fiat payment plugins** — Stripe, PayPal, Midtrans, Xendit; redirect-based checkout with webhooks.

All three paths support **affiliate referral tracking** via `?ref=USERNAME` in the watch URL.

---

## Payment Methods

| Provider | Status | Currency | Auto-disburse |
|---|---|---|---|
| **Internal Wallet** | ✅ Fully implemented | USD cents (ledger) | ✅ Instant (creator balance credited) |
| **x402 (Crypto / EVM)** | ✅ Fully implemented | MEGA, MATIC, ETH, USDC (ERC-20) | ✅ On-chain instant |
| **Stripe** | ✅ Implemented | USD, EUR, IDR | ❌ Needs Stripe Connect |
| **PayPal** | ✅ Implemented | USD, EUR, IDR | ❌ Needs Payouts API |
| **Midtrans** | ✅ Implemented | IDR | ❌ No payout API |
| **Xendit** | ✅ Implemented | IDR, PHP, USD | ✅ Auto-disburse to bank |

---

## Payment Methods

| Provider | Status | Currency | Auto-disburse |
|---|---|---|---|
| **x402 (Crypto / EVM)** | ✅ Fully implemented | MEGA, MATIC, ETH, USDC (ERC-20) | ✅ On-chain instant |
| **Stripe** | ✅ Implemented | USD, EUR, IDR | ❌ Needs Stripe Connect |
| **PayPal** | ✅ Implemented | USD, EUR, IDR | ❌ Needs Payouts API (creator PayPal email) |
| **Midtrans** | ✅ Implemented | IDR | ❌ No payout API |
| **Xendit** | ✅ Implemented | IDR, PHP, USD | ✅ Auto-disburse to bank (BCA, BNI, BRI, Mandiri, etc.) |

The active providers are controlled by the `PAYMENT_PLUGINS` environment variable (comma-separated list). The default is controlled by `PAYMENT_DEFAULT_PROVIDER`.

---

---

## Wallet Payment Flow

The internal wallet is the simplest path — no crypto wallet, no redirect, no waiting for webhooks.

```
Buyer selects "Wallet" tab on watch.html
    │
    ▼
POST /api/wallet/pay  { video_id, ref_code }
    │  - Verifies buyer is not owner, not already purchased
    │  - Checks buyer balance ≥ price_cents
    │  - Atomic DB transaction:
    │      → Deduct price_cents from buyer.balance_cents
    │      → Credit creator.balance_cents × CREATOR_SPLIT_BP / 10000
    │      → Insert wallet_transactions rows for buyer and creator
    │      → INSERT purchases + INSERT allowlist
    │  - After commit: process affiliate commission if ref_code is set
    │
    ▼
Buyer gets immediate access. Page reloads → video streams ✅
```

→ Full wallet documentation: [WALLET.md](WALLET.md)

---

## How x402 Payment Works (Step-by-Step)

### Does the buyer pay admin, who then forwards to owner?

**No.** The buyer pays **directly through a smart contract** (`X402Splitter.sol`). The contract splits the funds atomically in the same transaction — creator gets their cut instantly, admin gets theirs, with no manual steps in between.

### Full Flow

```
Buyer (MetaMask)
    │
    │  1. Click "Buy with Crypto" on /watch.html
    │
    ▼
Backend  POST /api/pay/x402/start
    │  - Creates invoice record in x402_invoices table
    │  - Signs the invoice with X402_ADMIN_PRIVKEY (EIP-712 / Ethereum signature)
    │  - Returns: invoice_uid, amount_wei, deadline, v, r, s (signature), contract address
    │
    ▼
Frontend (MetaMask / ethers.js)
    │  - Calls X402Splitter.payNativeSigned()  [for MATIC/MEGA/ETH]
    │     or X402Splitter.payERC20Signed()     [for USDC etc.]
    │  - Passes the signed invoice as arguments so the contract can verify it
    │
    ▼
Smart Contract  X402Splitter.sol  (on-chain)
    │  - Verifies admin's signature (prevents forged invoices)
    │  - Verifies invoice is not expired and not already used
    │  - Splits payment atomically:
    │      → 90% sent to Creator wallet
    │      → 10% sent to Admin wallet
    │  - Emits Paid(invoiceUid, payer, creator, admin, token, amountWei, videoId)
    │  - Marks invoice UID as used (replay protection)
    │
    ▼
Frontend
    │  - Polls POST /api/pay/x402/confirm every 2 seconds (up to 40 seconds)
    │
    ▼
Backend  POST /api/pay/x402/confirm
    │  - Fetches transaction receipt from blockchain RPC
    │  - Validates tx status = success (0x1)
    │  - Decodes and verifies the Paid event:
    │      • invoice_uid matches
    │      • video_id matches
    │      • amount ≥ min_amount_wei (underpay protection)
    │  - Marks invoice as paid in database
    │  - Inserts row into purchases table
    │  - Inserts row into allowlist table (grants permanent video access)
    │
    ▼
Buyer
    └─ Page reloads → video streams ✅
```

---

## Revenue Share

### Current Split

| Recipient | Share | Basis Points |
|---|---|---|
| **Creator (video owner)** | **90%** | 9000 bp |
| **Platform admin** | **10%** | 1000 bp |

Basis points: 10,000 = 100%. So 9000 bp = 90.00%.

### Where Is This Configured?

The split is controlled by **one environment variable** that flows to all three places:

```dotenv
CREATOR_SPLIT_BP=9000      # 9000 = 90%; admin gets 10000 - 9000 = 1000 bp = 10%
X402_DEADLINE_SECS=900     # payment window in seconds (default 15 min)
```

| Location | Uses |
|---|---|
| `src/config.rs` | Reads `CREATOR_SPLIT_BP` into `cfg.creator_split_bp` |
| `src/handlers/pay.rs` | Uses `st.cfg.creator_split_bp` and `st.cfg.x402_deadline_secs` |
| `src/plugins/payment/providers/x402.rs` | Reads `CREATOR_SPLIT_BP` / `X402_DEADLINE_SECS` from env (plugin has no cfg access) |
| `src/plugins/payment/providers/xendit.rs` | Reads `CREATOR_SPLIT_BP` to compute disburse amount |
| `contracts/contracts/X402Splitter.sol` | Accepts `creatorBp` as a parameter from the frontend — no redeployment needed |

### How to Change the Split

Set `CREATOR_SPLIT_BP` in your `.env` file and restart the server:

```dotenv
CREATOR_SPLIT_BP=8000   # 80% to creator, 20% to admin
```

No code changes or redeployment needed. The smart contract accepts `creatorBp` as a parameter so changing the env var is sufficient.

---

## Access Control After Payment

After a successful payment, access is granted by inserting a row into the `allowlist` table:

```sql
INSERT INTO allowlist (video_id, username)
VALUES ($1, $2)
ON CONFLICT (video_id, username) DO NOTHING
```

When a user tries to watch a video, the backend checks:

1. Is the user the **owner** of the video? → allow
2. Does `(video_id, username)` exist in **allowlist**? → allow
3. Otherwise → **403 Forbidden** (triggers the pay CTA in the browser)

This means access is **permanent** once granted — no expiry, no revocation unless the row is manually deleted.

Creators can also **manually grant access** without payment via `POST /api/allow` in the Dashboard.

---

## Configuration Reference (x402)

All x402 settings are loaded from environment variables (see [src/config.rs](src/config.rs)):

| Env Var | Description | Example |
|---|---|---|
| `X402_CONTRACT_ADDRESS` | Deployed `X402Splitter` contract address | `0xe375...AE8A` |
| `X402_ADMIN_WALLET` | Admin's EVM wallet address (receives 10%) | `0xB725...b6f0` |
| `X402_ADMIN_PRIVKEY` | Admin private key for signing invoices (**keep secret**) | `0x...` |
| `X402_RPC_HTTP` | HTTP JSON-RPC endpoint for tx confirmation | `https://polygon-amoy-bor.publicnode.com` |
| `X402_RPC_WSS` | WebSocket RPC for event watching | `wss://...` |
| `X402_CHAIN_ID` | Default EVM chain ID | `80002` |
| `PAYMENT_PLUGINS` | Active payment providers | `x402,stripe` |
| `PAYMENT_DEFAULT_PROVIDER` | Fallback provider | `x402` |
| `CREATOR_SPLIT_BP` | Creator share in basis points (0–10000) | `9000` (= 90%) |
| `X402_DEADLINE_SECS` | x402 payment window duration in seconds | `900` (= 15 min) |

### Supported Tokens & Chains

Tokens and chains are stored in the `pay_tokens` database table (seeded via [migrations/021_pay_tokens.sql](migrations/021_pay_tokens.sql)):

| Chain | Chain ID | Token | Type |
|---|---|---|---|
| Mega Testnet | 6342 | MEGA | Native |
| Polygon Amoy Testnet | 80002 | MATIC | Native |
| Polygon Mainnet | 137 | USDC | ERC-20 (`0x2791...4174`) |

New chains and tokens can be added by inserting rows into `pay_tokens` without code changes.

---

## Creator Wallet Setup

The creator must set their EVM wallet address in the Dashboard ([public/dashboard.html](public/dashboard.html)) under **Edit Profile → Creator EVM Wallet**. This must be a valid EVM address (`0x` + 40 hex chars).

If no wallet is set, the video cannot be purchased via x402 — the backend will reject invoice creation for that video.

The creator can also set a **Preferred Chain** (Mega Testnet, Polygon Mainnet, Ethereum Mainnet) which hints to buyers which chain to use, though buyers can choose any chain that has an active token listing.

---

---

## Fiat Payment Flow (Stripe / PayPal / Midtrans / Xendit)

All four fiat providers share the same database-backed flow:

```
Buyer clicks "Pay with [Provider]"
    │
    ▼
POST /api/pay/:provider/start
    │  - Pre-insert fiat_invoices (status = pending, invoice_uid = UUID)
    │  - Call provider API to create checkout/invoice
    │  - Update fiat_invoices with provider_ref and payment_url
    │  - Return payment_url to frontend
    │
    ▼
Browser redirects buyer to provider hosted page
    │
    ▼
Buyer completes payment on provider page
    │
    ▼
Provider sends webhook → POST /api/pay/:provider/webhook
    │  - Extract raw body bytes (needed for HMAC verification)
    │  - Verify signature:
    │    • Stripe: HMAC-SHA256 raw body vs STRIPE_WEBHOOK_SECRET
    │    • PayPal: POST to /v1/notifications/verify-webhook-signature API
    │    • Midtrans: SHA-512(order_id + status_code + gross_amount + server_key)
    │    • Xendit: x-callback-token header check
    │  - On PaymentStatus::Paid:
    │    → UPDATE fiat_invoices SET status='paid', paid_at=now()
    │    → INSERT purchases (user_id, video_id)
    │    → INSERT allowlist (video_id, username) — grants permanent access
    │    → [Xendit only] POST /disbursements → 90% to creator's bank account
    │
    ▼
Buyer now has permanent access to the video ✅
```

### Provider-Specific Details

| Provider | Checkout API | Webhook Verification | Creator Payout |
|---|---|---|---|
| **Stripe** | `POST /v1/checkout/sessions` → `url` | HMAC-SHA256 on raw bytes | Manual via Stripe dashboard |
| **PayPal** | `POST /v2/checkout/orders` → `approve` link | `/v1/notifications/verify-webhook-signature` | Manual via PayPal dashboard |
| **Midtrans** | `POST /snap/v1/transactions` → `redirect_url` | SHA-512 hash of concatenated fields | Manual (no Midtrans payout API) |
| **Xendit** | `POST /v2/invoices` → `invoice_url` | `x-callback-token` header | **Automatic** via Xendit Disbursements API |

### Creator Bank Account for Xendit

For Xendit auto-disburse to work, the creator must set their bank account in **Dashboard → Edit Profile**:

```
Format: BANK_CODE ACCOUNT_NUMBER a/n FULL_NAME
Example: BCA 1234567890 a/n Budi Santoso
```

The backend splits this string on `a/n` to extract the bank code, account number, and holder name for the Xendit Disbursements API call.

---

## Admin Payment Monitoring

The admin dashboard at `/public/admin/payments.html` provides:

- Filter by provider (Stripe/PayPal/Midtrans/Xendit) and status (pending/paid/failed)
- Total counts: all, paid, pending, failed
- Table showing invoice UID, buyer, video, amount, status, timestamps
- **Disburse button** for paid-but-not-yet-disbursed invoices:
  - Xendit: triggers real Disbursements API call
  - Others: marks `disburse_ref='manual'` (admin confirms they did it via provider dashboard)

API: `GET /admin/payments?provider=&status=&limit=`
Disburse: `POST /admin/payments/:invoice_uid/disburse`

---

## Affiliate Commission Integration

All three payment paths propagate the `?ref=USERNAME` referral code:

| Path | How ref is captured | When commission is paid |
|------|---------------------|------------------------|
| Wallet | `ref_code` in POST body | Immediately after purchase commits |
| X402 | `ref_code` in start body → stored as `x402_invoices.affiliate_ref` | After `x402/confirm` grants access |
| Fiat | `affiliate_ref` in start body → stored as `fiat_invoices.affiliate_ref` | After provider webhook confirms payment |

Commission is `price_cents × commission_pct / 100`, deducted from creator's wallet balance and credited to affiliate's wallet balance. Requires `affiliate_settings.is_enabled = true` for the video.

→ Full affiliate documentation: [AFFILIATE.md](AFFILIATE.md)

---

## Summary

```
Wallet: Buyer balance → Creator balance (instant, atomic)
X402:   Buyer → Smart contract → Creator gets 0.9X instantly, Admin gets 0.1X
Fiat:   Buyer → Provider hosted page → Webhook → Backend grants access → Admin triggers disburse
```

- **Revenue share: 90% creator / 10% admin** — enforced in smart contract (x402), creator wallet credit (wallet), or admin-triggered disburse (fiat)
- **Access: permanent** — stored in `allowlist` table
- **Affiliate commission** — creator-funded; paid after every confirmed sale

---

## Related Documentation

- [README.md](README.md) — platform overview
- [WALLET.md](WALLET.md) — internal wallet system
- [AFFILIATE.md](AFFILIATE.md) — affiliate and commission system
- [PAYMENT_PLUGIN_ARCHITECTURE.md](PAYMENT_PLUGIN_ARCHITECTURE.md) — fiat plugin internals
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) — code-level reference
