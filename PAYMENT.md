# Payment System — How It Works

## Overview

This platform is a **Pay-Per-View (PPV) streaming service**. Buyers pay directly to access a specific video. Revenue is automatically split between the **creator (owner)** and the **platform admin** at the moment of payment.

Two payment paths are available:

1. **x402 (on-chain crypto)** — primary; funds split on-chain via smart contract, instant.
2. **Fiat payment plugins** — Stripe, PayPal, Midtrans, Xendit; all fully implemented with webhook receivers and automatic DB access grants.

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

The split is **hardcoded in two places in the source code**. There is no env var or config file for it — changing it requires editing the source and redeploying.

**Location 1 — Rust handler** ([src/handlers/pay.rs](src/handlers/pay.rs)):
```rust
let creator_basis_points: u16 = 9000;  // 90% to creator
// split_admin_bp is implicitly 10000 - 9000 = 1000
```

**Location 2 — x402 plugin** ([src/plugins/payment/providers/x402.rs](src/plugins/payment/providers/x402.rs)):
```rust
"split_creator_bp": 9000,  // sent to frontend so MetaMask knows the split
"split_admin_bp":   1000,
```

**Location 3 — Smart contract** ([contracts/contracts/X402Splitter.sol](contracts/contracts/X402Splitter.sol)):
```solidity
uint256 toCreator = (msg.value * creatorBp) / BP_DENOM;  // creatorBp = 9000
uint256 toAdmin   = msg.value - toCreator;               // remainder = 10%
```

The `creatorBp` value is passed into the contract function call by the frontend, taken from the backend's response. The smart contract enforces the value is within an acceptable range.

### How to Change the Split

1. Edit `creator_basis_points` in `src/handlers/pay.rs`
2. Edit `split_creator_bp` in `src/plugins/payment/providers/x402.rs`
3. Rebuild and redeploy the Rust backend
4. The contract itself accepts `creatorBp` as a parameter — no contract redeployment needed for split changes, only backend changes

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

## Summary

```
x402: Buyer → Smart contract → Creator gets 0.9X instantly, Admin gets 0.1X
Fiat: Buyer → Provider hosted page → Webhook → Backend grants access → Admin triggers disburse
```

- **Revenue share: 90% creator / 10% admin** — enforced in smart contract (x402) or admin-triggered disburse (fiat)
- **Access: permanent** — stored in `allowlist` table
- **Xendit only**: auto-disburse to creator's bank via Xendit API on webhook receipt
- **All others**: admin manually transfers via provider dashboard, then clicks Disburse in admin panel
