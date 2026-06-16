# Mini Wallet — Business & Technical Guide

> Internal fiat wallet for PPV Stream Rust.  
> Pure database ledger — no blockchain, no third-party payment processor required.

---

## Table of Contents

1. [Business Overview](#1-business-overview)
2. [Actor Roles](#2-actor-roles)
3. [Transaction Types](#3-transaction-types)
4. [Business Flows](#4-business-flows)
   - [Deposit](#41-deposit-flow)
   - [Withdrawal (Payout)](#42-withdrawal-payout-flow)
   - [Peer-to-Peer Transfer](#43-peer-to-peer-transfer-flow)
5. [Business Rules & Limits](#5-business-rules--limits)
6. [Technical Architecture](#6-technical-architecture)
7. [Database Design](#7-database-design)
8. [API Reference](#8-api-reference)
9. [Security Model](#9-security-model)
10. [Operational Playbook](#10-operational-playbook)

---

## 1. Business Overview

The mini wallet is an **internal balance system** built into the platform.

Every user has a balance (stored in cents, e.g. `5000` = $50.00). Users can:

- **Top up** their balance by requesting a deposit — the admin manually verifies incoming money (bank transfer, cash, e-wallet) and approves the credit.
- **Pay out** their balance by requesting a withdrawal — the admin manually sends the money to the user's bank account or e-wallet and marks the transaction as completed.
- **Send money to other users** instantly without admin involvement.

This model is common in marketplace and creator platforms:

| Platform analogy | How they do it |
|---|---|
| OVO / GoPay / Dana | Internal balance, top up via bank transfer or merchant |
| Tokopedia Saldo | Seller earnings accumulate, can be withdrawn to bank |
| OnlyFans balance | Creator balance from subscriptions, paid out weekly |
| PPV Stream Wallet | Same pattern — self-operated, no payment processor fee |

**The platform operator is the "bank."** They hold responsibility for verifying deposits and executing payouts manually. In return, there are no payment gateway fees on internal transfers — only on the initial deposit channel (which can be anything: BCA transfer, GoPay, cash, etc.).

---

## 2. Actor Roles

```
┌─────────────────────────────────────────────────────────────────┐
│                         PPV Stream                              │
│                                                                 │
│   ┌───────────┐    transfer    ┌───────────┐                   │
│   │  User A   │ ─────────────► │  User B   │                   │
│   │ (Viewer)  │                │ (Creator) │                   │
│   └─────┬─────┘                └─────┬─────┘                   │
│         │ deposit/withdraw           │ deposit/withdraw        │
│         ▼                            ▼                         │
│   ┌──────────────────────────────────────────┐                 │
│   │               Admin                      │                 │
│   │  - Approves deposits (credits balance)   │                 │
│   │  - Processes withdrawals (sends money)   │                 │
│   │  - Rejects invalid requests              │                 │
│   └──────────────────────────────────────────┘                 │
└─────────────────────────────────────────────────────────────────┘
```

| Actor | Responsibilities |
|---|---|
| **User** | Requests deposits, requests withdrawals, transfers to other users, views own history |
| **Admin** | Approves deposits after verifying payment, processes payouts, rejects invalid requests |
| **Platform** | Holds the ledger, enforces balance integrity, records every mutation |

---

## 3. Transaction Types

Every wallet event produces exactly one or two rows in `wallet_transactions`.

| `txn_type` | Who triggers | Admin needed? | Balance effect |
|---|---|---|---|
| `deposit` | User requests | Yes — admin approves | +balance when approved |
| `withdrawal` | User requests | Yes — admin processes | -balance immediately (held pending) |
| `transfer_out` | User sends | No | -balance immediately |
| `transfer_in` | System (recipient side) | No | +balance immediately |

### Transaction Status Lifecycle

```
deposit:
  [user submits] → pending → approved  (balance credited)
                           → rejected  (no change)

withdrawal:
  [user submits] → pending → completed (payout sent, balance already deducted)
                           → rejected  (balance refunded)

transfer:
  [instant]      → completed  (atomic, both sides)
```

---

## 4. Business Flows

### 4.1 Deposit Flow

**Goal:** User adds money to their platform wallet.

```
User                       Platform                      Admin
 │                             │                           │
 │  1. Transfers money         │                           │
 │     (bank / e-wallet)       │                           │
 │                             │                           │
 │  2. POST /api/wallet/deposit│                           │
 │     {amount_cents, note}    │                           │
 │ ──────────────────────────► │                           │
 │                             │ 3. Creates pending row    │
 │                             │    in wallet_transactions │
 │  4. "Request submitted"     │                           │
 │ ◄────────────────────────── │                           │
 │                             │                           │
 │                             │ 5. Admin checks inbox /   │
 │                             │    bank statement         │
 │                             │                           │
 │                             │ 6. POST /admin/wallet/    │
 │                             │    transactions/:id/approve◄────────
 │                             │                           │
 │                             │ 7. balance_cents += amount│
 │                             │    status = 'approved'    │
 │                             │                           │
 │  8. Balance updated ✓       │                           │
```

**User experience:**
1. User initiates bank transfer of $50 to the platform's bank account.
2. User fills in the deposit form: amount = 5000 cents, note = "BCA transfer 12345".
3. Admin sees the pending request in `/public/admin/wallet.html`.
4. Admin verifies bank statement matches the claimed amount.
5. Admin clicks **Approve** — user's balance is instantly credited.

**What if the transfer never arrives?**
Admin clicks **Reject** with a note. No balance change occurs. User is notified via the status change in their transaction history.

---

### 4.2 Withdrawal (Payout) Flow

**Goal:** User withdraws their balance to their bank account or e-wallet.

```
User                       Platform                      Admin
 │                             │                           │
 │  1. POST /api/wallet/withdraw                           │
 │     {amount_cents, note:    │                           │
 │      "BCA 1234567890/John"} │                           │
 │ ──────────────────────────► │                           │
 │                             │ 2. Checks balance ≥ amount│
 │                             │    balance -= amount      │
 │                             │    (balance HELD)         │
 │                             │    Creates pending row    │
 │  3. "Request submitted,     │                           │
 │      balance deducted"      │                           │
 │ ◄────────────────────────── │                           │
 │                             │                           │
 │                             │ 4. Admin processes payout │
 │                             │    (sends via bank/GoPay) │
 │                             │                           │
 │                             │ 5. POST /admin/wallet/    │
 │                             │    transactions/:id/complete◄──────
 │                             │    status = 'completed'   │
 │  6. Payout received ✓       │                           │
```

**Why is balance deducted immediately (step 2)?**

To prevent the user from submitting multiple withdrawal requests that exceed their actual balance. The held balance is the platform's liability — it will be paid out or refunded.

**What if admin rejects?**
Admin clicks **Reject & Refund**. The system automatically adds the held amount back to the user's balance (`balance_cents += amount`). The user sees status `rejected` with the admin note.

---

### 4.3 Peer-to-Peer Transfer Flow

**Goal:** User A sends money directly to User B. Instant, no admin needed.

```
User A                     Platform                     User B
 │                             │                           │
 │  POST /api/wallet/transfer  │                           │
 │  {to_username: "bob",       │                           │
 │   amount_cents: 2000,       │                           │
 │   note: "split bill"}       │                           │
 │ ──────────────────────────► │                           │
 │                             │ 1. Verify sender balance  │
 │                             │    ≥ 2000 cents           │
 │                             │                           │
 │                             │ 2. BEGIN TRANSACTION      │
 │                             │    A.balance -= 2000      │
 │                             │    B.balance += 2000      │
 │                             │    ledger: transfer_out   │
 │                             │    ledger: transfer_in    │
 │                             │    COMMIT                 │
 │                             │                           │
 │  3. "Sent $20.00 to @bob"   │                           │
 │ ◄────────────────────────── │                           │
 │                             │ ──────────────────────── ►│
 │                             │    4. @bob sees +$20.00   │
 │                             │       in their history    │
```

**Key characteristic: atomic transaction.**
Either both sides succeed, or neither does. There is no state where money disappears mid-transfer.

---

## 5. Business Rules & Limits

| Rule | Value | Reason |
|---|---|---|
| Minimum deposit | $10.00 (1,000 cents) | Admin overhead not worth smaller amounts |
| Minimum withdrawal | $50.00 (5,000 cents) | Transfer fee on the payout side is relatively fixed |
| Minimum transfer | $1.00 (100 cents) | Prevent spam micro-transactions |
| Balance floor | $0.00 | Cannot go negative under any circumstance |
| Withdrawal balance hold | Immediate | Prevents double-spend |
| Transfer direction | Always explicit | Cannot transfer to self |
| Deposit approval | Manual by admin | Platform verifies real money arrived |
| Payout execution | Manual by admin | Admin sends money via external channel |

---

## 6. Technical Architecture

### Data Flow Overview

```
Browser (wallet.html)
        │
        │  JSON over HTTPS
        ▼
Axum HTTP Server (Rust)
  handlers/wallet.rs
        │
        │  sqlx async queries
        ▼
PostgreSQL
  users.balance_cents     ← source of truth for current balance
  wallet_transactions     ← immutable audit ledger
```

### Concurrency Safety

All balance mutations use **PostgreSQL row-level locks** (`SELECT ... FOR UPDATE`) inside an explicit transaction. This prevents:

- **Double-spend:** Two simultaneous withdrawals cannot both succeed if only one has sufficient balance.
- **Transfer deadlock:** Rows are always locked in consistent alphabetical order by `user_id` — so A→B and B→A simultaneous transfers cannot deadlock each other.

```
Thread 1: transfer A→B           Thread 2: transfer B→A
  LOCK users WHERE id='alice'      LOCK users WHERE id='alice'  ← waits
  LOCK users WHERE id='bob'
  UPDATE alice.balance -= x
  UPDATE bob.balance += x
  COMMIT
                                   ← lock acquired
                                   LOCK users WHERE id='bob'
                                   UPDATE bob.balance -= y
                                   UPDATE alice.balance += y
                                   COMMIT
```

Because both threads lock in the same order (`alice` before `bob`), Thread 2 simply waits for Thread 1 to finish. No deadlock.

---

## 7. Database Design

### Column added to `users`

```sql
ALTER TABLE users ADD COLUMN balance_cents BIGINT NOT NULL DEFAULT 0;
```

`balance_cents` is always the **current live balance**. It is the authoritative number shown to users and checked before any deduction.

### `wallet_transactions` table

```sql
CREATE TABLE wallet_transactions (
    id            BIGSERIAL PRIMARY KEY,
    user_id       TEXT        NOT NULL REFERENCES users(id),
    txn_type      TEXT        NOT NULL,  -- deposit | withdrawal | transfer_in | transfer_out
    amount_cents  BIGINT      NOT NULL CHECK (amount_cents > 0),
    balance_after BIGINT      NOT NULL DEFAULT 0,  -- snapshot after this event
    status        TEXT        NOT NULL DEFAULT 'pending',
    ref_user_id   TEXT        REFERENCES users(id), -- counterparty (transfers)
    note          TEXT,                              -- user-supplied description
    admin_note    TEXT,                              -- admin action reason
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Two sources, one truth

| Field | Purpose | Mutated by |
|---|---|---|
| `users.balance_cents` | Live balance — what the user can spend right now | Every approve/transfer/withdraw |
| `wallet_transactions.balance_after` | Snapshot at the time of each event — historical audit trail | Set once on insert (approve updates it) |

Together they let you reconstruct balance history: `balance_after` in each row shows what the balance was immediately after that event fired.

### Entity Relationship

```
users ──────────────────────────────────────┐
  id  PK                                    │
  balance_cents  ← live balance             │
      │                                     │
      │ 1:N                                 │
      ▼                                     │
wallet_transactions                         │
  id             PK                         │
  user_id        FK → users(id)             │
  ref_user_id    FK → users(id) (nullable) ←┘
  txn_type       deposit|withdrawal|transfer_in|transfer_out
  amount_cents   always positive
  balance_after  snapshot
  status         pending|approved|completed|rejected
```

---

## 8. API Reference

### User endpoints

All require a valid session cookie (`ppv_session`).

#### `GET /api/wallet/balance`

Returns current balance.

```json
{
  "ok": true,
  "balance_cents": 12500,
  "balance_display": "$125.00"
}
```

#### `GET /api/wallet/transactions?limit=50`

Returns transaction history, newest first.

```json
{
  "ok": true,
  "items": [
    {
      "id": 42,
      "txn_type": "deposit",
      "amount_cents": 5000,
      "balance_after": 12500,
      "status": "approved",
      "ref_username": null,
      "note": "BCA transfer 28 Jun",
      "admin_note": "Verified, amount matches",
      "created_at": "2026-06-16 09:23:00"
    }
  ]
}
```

#### `POST /api/wallet/deposit`

```json
// Request
{ "amount_cents": 5000, "note": "BCA transfer ref 12345" }

// Response (success)
{ "ok": true, "txn_id": 42, "message": "Deposit request submitted. Awaiting admin approval." }

// Response (below minimum)
{ "ok": false, "error": "minimum deposit is $10.00" }
```

#### `POST /api/wallet/withdraw`

```json
// Request
{ "amount_cents": 5000, "note": "BCA 1234567890 / John Doe" }

// Response (success — balance already deducted)
{ "ok": true, "txn_id": 43, "balance_cents": 7500, "balance_display": "$75.00",
  "message": "Withdrawal request submitted. Admin will process your payout." }

// Response (insufficient)
{ "ok": false, "error": "insufficient balance ($75.00)" }
```

#### `POST /api/wallet/transfer`

```json
// Request
{ "to_username": "alice", "amount_cents": 2000, "note": "split dinner" }

// Response (success)
{ "ok": true, "balance_cents": 5500, "balance_display": "$55.00",
  "transferred_to": "alice", "amount_display": "$20.00" }

// Response (recipient not found)
{ "ok": false, "error": "recipient not found" }
```

---

### Admin endpoints

No session check at the route level — protect via network/reverse proxy or add session middleware as needed.

#### `GET /admin/wallet/transactions?txn_type=deposit&status=pending&limit=100`

```json
{
  "ok": true,
  "totals": { "all": 120, "pending": 5 },
  "items": [
    {
      "id": 42,
      "username": "john",
      "email": "john@example.com",
      "txn_type": "deposit",
      "amount_cents": 5000,
      "balance_after": 5000,
      "status": "pending",
      "note": "BCA transfer ref 12345",
      "admin_note": null,
      "ref_username": null,
      "created_at": "2026-06-16 09:23:00"
    }
  ]
}
```

#### `POST /admin/wallet/transactions/:id/approve`

Approves a `deposit`. Credits `amount_cents` to the user's `balance_cents`. Updates `wallet_transactions.status` to `approved` and sets `balance_after` to the new real balance.

```json
// Request body (admin_note is optional)
{ "admin_note": "Verified via BCA statement 16 Jun" }

// Response
{ "ok": true, "new_balance_cents": 5000 }
```

#### `POST /admin/wallet/transactions/:id/complete`

Marks a `withdrawal` as completed. Money has been sent by admin externally. Does not touch `balance_cents` (already deducted at request time).

```json
{ "admin_note": "Sent via GoPay 09:45" }
// Response
{ "ok": true }
```

#### `POST /admin/wallet/transactions/:id/reject`

Rejects any `pending` transaction.
- If `deposit`: no balance change.
- If `withdrawal`: `balance_cents += amount_cents` (refund).

```json
{ "admin_note": "Transfer not found in bank statement" }
// Response
{ "ok": true, "refunded": false }   // deposit rejection
{ "ok": true, "refunded": true  }   // withdrawal rejection + refund
```

---

## 9. Security Model

### Balance integrity

The single line that guarantees no money is created or destroyed:

```
deposit approved:    balance += amount          (admin action, verified manually)
withdrawal request:  balance -= amount          (held; refunded if rejected)
transfer:            sender.balance -= amount
                     recipient.balance += amount (same DB transaction, atomic)
```

The sum of all `balance_cents` across all users equals the total liability the platform owes to its users. This must match the platform's actual cash holdings in the real world.

### What the system does NOT protect against

| Risk | Mitigation required |
|---|---|
| Admin approving a deposit that never arrived | Admin manual verification process |
| Admin processing a withdrawal to the wrong account | Admin double-checks note/bank account |
| Unauthorized admin API access | Add session/auth middleware to admin routes |
| User submitting a fake deposit note | Admin verifies against real bank statement |

### Cookie-based auth

User endpoints check the HMAC-SHA256 signed `ppv_session` cookie via `sessions::current_user_id()`. An expired or tampered cookie returns `401 not logged in`. No wallet action can proceed without a valid session.

---

## 10. Operational Playbook

### Daily admin checklist

```
1. Open /public/admin/wallet.html
2. Filter by txn_type=deposit, status=pending
3. For each pending deposit:
   a. Open bank statement / e-wallet history
   b. Find matching transfer (amount + date matches)
   c. Click Approve with note "Verified: [reference]"
   d. If not found within 24h → Reject with note "Transfer not received"

4. Filter by txn_type=withdrawal, status=pending
5. For each pending withdrawal:
   a. Read note field for user's bank account / destination
   b. Send money via banking app or e-wallet
   c. Click Mark Paid with note "Sent: [receipt number]"
   d. If user's destination is invalid → Reject with note → balance refunded
```

### How to handle disputes

A user claims their deposit was not credited:
1. Find the transaction by ID in the admin wallet table.
2. Check `status` — if `pending`, the deposit is awaiting approval.
3. Check `admin_note` — if `rejected`, the reason is there.
4. Cross-check against the bank statement for the date and amount.
5. If legitimate, approve. If already approved but balance looks wrong, check `balance_after` for that row.

### Balance reconciliation

At any point, the expected total platform liability is:

```sql
SELECT SUM(balance_cents) FROM users;
```

This must not exceed the real money the platform holds across all its accounts. Run this check weekly.

To see full ledger history for a specific user:

```sql
SELECT id, txn_type, amount_cents, balance_after, status, note, admin_note, created_at
FROM wallet_transactions
WHERE user_id = '<user_id>'
ORDER BY created_at;
```

The last row's `balance_after` should match `users.balance_cents` for that user.

---

## Summary

```
Money In  → Bank Transfer → Admin Verifies → Deposit Approved → balance_cents ↑
Money Out → User Requests → balance held   → Admin Sends     → Mark Completed
Internal  → User Transfers → Atomic swap   → No admin needed → Instant
```

The wallet is a **trust-based internal ledger**. Its strength is simplicity — no API keys, no webhook callbacks, no third-party dependencies. Its operational cost is manual admin effort for every deposit and withdrawal. For platforms with a moderate transaction volume and a trusted admin, this is the most cost-effective and controllable model available.
