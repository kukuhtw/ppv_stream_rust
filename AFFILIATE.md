# Affiliate System — PPV Stream

→ [README.md](README.md) | [WALLET.md](WALLET.md) | [PAYMENT.md](PAYMENT.md) | [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md)

## Overview

The affiliate system lets creators grow their video sales by incentivising other users to promote their content. A creator configures a commission percentage per video; affiliates share unique referral links; when a buyer purchases through a referral link, the affiliate earns a commission automatically — no admin approval needed, no smart contract, no blockchain. Everything is settled as ledger entries in the internal wallet.

---

## How It Works — Business Perspective

### Actors

| Actor | Role | What they get |
|-------|------|---------------|
| **Creator (User A)** | Owns the video. Decides commission % | Pays out commission from their revenue share |
| **Affiliate (User B)** | Promotes the video via a referral link | Earns `commission_pct %` of the video price on every sale they drive |
| **Buyer (User C)** | Purchases the video | Normal purchase experience — price is unchanged |
| **Platform** | Facilitates and records all flows | Retains its platform fee; commissions are between creator and affiliate |

### The Flow in Plain Language

1. **Creator enables affiliate program** on a video and sets `commission_pct` (e.g. `10%`, max `90%`).
2. **Affiliate navigates** to `/public/affiliate.html`, selects the video, and copies their unique referral link:
   ```
   https://platform.com/public/watch.html?video_id=VIDEO_ID&ref=affiliate_username
   ```
3. **Affiliate shares the link** — via social media, email, blog post, etc.
4. **Buyer clicks the link** and lands on the watch page. They see a "Referred by @affiliate_username" notice.
5. **Buyer purchases** using any available payment method (wallet, crypto X402, or fiat gateway).
6. **System awards commission automatically:**
   - `commission_cents = video_price * commission_pct / 100`
   - This amount is **deducted from the creator's wallet balance** and **credited to the affiliate's wallet balance**.
   - Both the creator and affiliate receive a wallet transaction notification (visible in `/public/wallet.html`).
7. **Affiliate withdraws** earnings via the standard wallet withdrawal flow when they're ready.

### Commission Source

The commission comes from the **creator's revenue share**, not from the buyer's payment or the platform fee:

```
Buyer pays:        $10.00  (video price)
Platform retains:  $ 1.00  (10% platform fee, example)
Creator receives:  $ 9.00  (90% split, example)
  ↓ creator pays affiliate commission
Affiliate earns:   $ 1.00  (10% of $10 price = creator gives up $1 of their $9)
Creator nets:      $ 8.00  (after commission)
```

> The creator decides how much of their revenue they're willing to share. The platform fee is unaffected.

### Commission Timing

| Payment Method | When commission is paid |
|----------------|------------------------|
| Wallet | Immediately after the purchase transaction commits |
| Crypto (X402) | After on-chain payment is verified and access is granted |
| Fiat (Stripe/PayPal/Midtrans/Xendit) | After the payment provider webhook confirms payment |

### Commission Timing vs Creator Disbursement

Affiliate commission timing is **not always the same** as creator disbursement timing.

| Payment Method | When creator gets sale proceeds | When affiliate gets commission | Admin manual action needed? |
|----------------|--------------------------------|-------------------------------|-----------------------------|
| Wallet | Immediately in internal wallet balance | Immediately after purchase commit | No |
| X402 | Immediately on-chain to creator EVM wallet | After backend confirms the on-chain payment | No creator disburse action |
| Stripe / PayPal / Midtrans | Later, after admin handles payout manually | After provider webhook confirms payment | Yes, creator disbursement is manual |
| Xendit | Usually right after webhook through Xendit Disbursements API | After provider webhook confirms payment | Usually no, but admin can retry payout if auto-disburse fails |

This means a referred sale can already create affiliate earnings even when the creator payout is still pending manual disbursement on a fiat provider.

### Safety Rules

- **Affiliate ≠ Buyer**: Self-referral is blocked. If the buyer and affiliate are the same account, no commission is paid.
- **Affiliate ≠ Creator**: The video owner cannot earn commission on their own video.
- **Insufficient balance**: If the creator's wallet balance is lower than the commission amount, the commission is skipped silently. The purchase still succeeds.
- **Program disabled**: If the creator has `is_enabled = false`, no commission is paid regardless of the `?ref=` parameter.
- **Non-existent referrer**: If the `?ref=` username does not exist in the platform, the commission is silently skipped.

---

## How It Works — Technical Perspective

### Database Schema (Migration `029_affiliate.sql`)

```sql
-- Creator configures commission per video
CREATE TABLE affiliate_settings (
    video_id       TEXT    PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE,
    owner_id       TEXT    NOT NULL REFERENCES users(id),
    commission_pct INT     NOT NULL DEFAULT 0 CHECK (commission_pct >= 0 AND commission_pct <= 90),
    is_enabled     BOOLEAN NOT NULL DEFAULT false,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Immutable audit log of every commission paid
CREATE TABLE affiliate_commissions (
    id                   BIGSERIAL   PRIMARY KEY,
    video_id             TEXT        NOT NULL REFERENCES videos(id),
    affiliate_id         TEXT        NOT NULL REFERENCES users(id),  -- User B
    buyer_id             TEXT        NOT NULL REFERENCES users(id),  -- User C
    owner_id             TEXT        NOT NULL REFERENCES users(id),  -- User A
    purchase_price_cents BIGINT      NOT NULL,
    commission_cents     BIGINT      NOT NULL,
    payment_method       TEXT        NOT NULL DEFAULT 'wallet',
    ref_invoice_uid      TEXT,       -- links to x402_invoices or fiat_invoices
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Referral is stored on the invoice at creation time
ALTER TABLE x402_invoices ADD COLUMN affiliate_ref TEXT;
ALTER TABLE fiat_invoices  ADD COLUMN affiliate_ref TEXT;
```

The `affiliate_ref` column in invoice tables is the **affiliate username captured at the moment the invoice is created** — before payment is confirmed. This prevents race conditions where the URL param might change between invoice creation and payment verification.

### Core Module: `src/commission.rs`

```rust
pub async fn process_affiliate_commission(
    pool:               &PgPool,
    video_id:           &str,
    buyer_id:           &str,
    owner_id:           &str,
    price_cents:        i64,
    affiliate_username: &str,
    payment_method:     &str,
    invoice_uid:        Option<&str>,
) -> Result<i64, String>
```

Steps executed inside a single DB transaction:

1. **Load `affiliate_settings`** for `video_id` — if no row or `is_enabled = false` or `commission_pct = 0`, return `Ok(0)`.
2. **Resolve affiliate** by `username` → get `affiliate_id`.
3. **Validate**: `affiliate_id ≠ buyer_id`, `affiliate_id ≠ owner_id`.
4. **Calculate**: `commission_cents = price_cents * commission_pct / 100`.
5. **Lock rows** in alphabetical order by `id` (`SELECT … FOR UPDATE`) to prevent deadlocks.
6. **Check creator balance** ≥ `commission_cents`; skip if insufficient.
7. **Deduct** `commission_cents` from `creator.balance_cents`.
8. **Credit** `commission_cents` to `affiliate.balance_cents`.
9. **Insert** two `wallet_transactions` rows (one `transfer_out` for creator, one `transfer_in` for affiliate).
10. **Insert** one `affiliate_commissions` row.
11. **Commit**.

Returns `Ok(commission_cents)` on success, `Ok(0)` when skipped, `Err(String)` only on unexpected DB failure.

### Payment Path Integration

The `?ref=` parameter is captured once in the browser and passed through all three payment paths:

#### Wallet Payment (`POST /api/wallet/pay`)

```json
{ "video_id": "xyz", "ref_code": "affiliate_username" }
```

After the atomic purchase transaction commits (buyer debited, creator credited), `process_affiliate_commission` is called as a best-effort step in a separate transaction.

Disbursement behavior for wallet purchases:

- creator share is already credited before affiliate commission runs
- affiliate commission then reduces the creator's internal wallet balance
- net creator wallet result = creator split minus affiliate commission
- no admin disbursement step exists for this sale path

#### Crypto X402 (`POST /api/pay/x402/start` + `POST /api/pay/x402/confirm`)

```json
// start: creates invoice
{ "video_id": "xyz", ..., "ref_code": "affiliate_username" }
```

The `affiliate_ref` is stored on the `x402_invoices` row at invoice creation. At payment confirmation (`x402_confirm`), after access is granted, the handler reads `affiliate_ref` back from the DB and calls the commission helper.

Disbursement behavior for x402 purchases:

- the creator sale proceeds are paid directly on-chain by the smart contract
- the affiliate commission is still paid from the creator's **internal platform wallet balance**
- therefore, creator payout and affiliate payout happen on two different rails
- if the creator has insufficient internal wallet balance, affiliate commission can be skipped even though the creator already received the on-chain sale proceeds

#### Fiat Gateway (`POST /api/pay/:provider/start` + webhook)

```json
// start: creates invoice
{ "video_id": "xyz", ..., "affiliate_ref": "affiliate_username" }
```

Same pattern: `affiliate_ref` is stored on the `fiat_invoices` row at invoice creation. The webhook handler (called by Stripe/PayPal/Midtrans/Xendit) reads `affiliate_ref` after confirming payment and calls the commission helper.

Disbursement behavior for fiat purchases:

- buyer access is granted after webhook confirmation
- affiliate commission is attempted immediately after webhook confirmation
- creator payout depends on the provider:
  - Stripe / PayPal / Midtrans: manual disbursement later by admin
  - Xendit: automatic disbursement attempt via Xendit API

So a creator can have:

- affiliate commission already deducted
- buyer already unlocked
- creator bank payout still pending

for manual-disburse fiat providers.

### Handler Endpoints (`src/handlers/affiliate.rs`)

| Method | Route | Description | Who |
|--------|-------|-------------|-----|
| `GET`  | `/api/affiliate/settings?video_id=` | Read current settings for a video | Creator |
| `POST` | `/api/affiliate/settings` | Upsert commission % and enabled flag | Creator |
| `GET`  | `/api/affiliate/link?video_id=` | Get referral link for current user | Any user |
| `GET`  | `/api/affiliate/earnings` | List commissions earned by current user | Affiliate |
| `GET`  | `/api/affiliate/program?video_id=` | Public: is affiliate program active? | Anyone |
| `GET`  | `/admin/affiliate/commissions` | All commissions on the platform | Admin |

### Frontend (`public/affiliate.html`)

Three-tab interface:

- **My Earnings** — total commission earned, paginated history table (video, buyer, amount, payment method, date).
- **My Videos** — creator view; select a video, set `commission_pct`, toggle `is_enabled`, save.
- **Get Links** — any user can generate their referral link for any video; shows whether the program is active.

### Referral Tracking in `watch.html`

```js
// Captured once on page load:
REF_CODE = new URLSearchParams(location.search).get('ref') || '';

// Included in every payment call:
{ video_id, ref_code: REF_CODE }       // wallet
{ video_id, ..., ref_code: REF_CODE }  // x402
{ video_id, ..., affiliate_ref: REF_CODE }  // fiat
```

A referral notice badge is rendered in the payment panel when `REF_CODE` is non-empty.

---

## Security Model

- **Commission is creator-funded**: affiliates cannot drain platform or buyer balances.
- **Row-level locking**: the same deadlock-prevention pattern used by the wallet transfer (alphabetical lock order) prevents concurrent commission transactions from conflicting.
- **Best-effort semantics**: commission failure never rolls back a purchase. A buyer always gets access even if the commission step has a transient error.
- **Idempotency**: commission is tied to a single purchase event. Since `affiliate_ref` is stored on the invoice at creation, replaying the webhook does not create a duplicate commission (the `purchases` insert uses `ON CONFLICT DO NOTHING`).
- **Input validation**: `commission_pct` is server-side constrained to `0–90`. The `affiliate_username` is looked up by exact DB match — it cannot be spoofed to an arbitrary wallet.

### Best-Effort Rule During Disbursement

When a referred purchase succeeds, the system prioritises **buyer access first**.

That means:

- the buyer should not lose access just because affiliate commission failed
- the creator disbursement method should not block access once payment is confirmed
- affiliate settlement can be skipped if the creator's internal wallet does not contain enough balance for the commission deduction

This is especially important for:

- **x402**, where creator sale payout happens on-chain but affiliate settlement is off-chain
- **Stripe / PayPal / Midtrans**, where creator payout may still be pending manual disbursement

---

## Admin Operational View

Access `/admin/affiliate/commissions` to see a full ledger:

```json
{
  "ok": true,
  "total_commission_cents": 4200,
  "total_display": "$42.00",
  "count": 12,
  "items": [
    {
      "affiliate_username": "bob",
      "owner_username":     "alice",
      "buyer_username":     "carol",
      "video_title":        "Advanced Rust Programming",
      "commission_cents":   500,
      "commission_display": "$5.00",
      "purchase_price_cents": 5000,
      "payment_method":     "wallet",
      "created_at":         "2026-06-16T10:23:45Z"
    }
  ]
}
```

---

## Example Scenario

1. Alice uploads a tutorial priced at **$20.00**.
2. Alice goes to **Affiliate → My Videos**, sets `commission_pct = 15`, enables the program.
3. Bob goes to **Affiliate → Get Links**, selects Alice's tutorial, copies:
   ```
   https://platform.com/public/watch.html?video_id=tutorial_abc&ref=bob
   ```
4. Bob posts the link on Twitter.
5. Carol clicks the link, sees "Referred by @bob", buys with her wallet balance.
6. System processes:
   - Carol's wallet: `−$20.00`
   - Alice's wallet: `+$18.00` (90% platform split) `−$3.00` (affiliate commission) = **+$15.00 net**
   - Bob's wallet: `+$3.00` (15% of $20)
   - Platform retains: `$2.00` (10% platform fee)
7. Bob's Earnings dashboard shows `$3.00` commission with video title and buyer.
