# Provider Settlement Guide

## Overview

When a viewer from instance A (the **traffic provider**) purchases a video
on instance B (this instance), instance B optionally credits a percentage
of the sale to instance A.  Settlement is currently off-chain: instance B
tracks the amounts owed and transfers them outside the platform (wire
transfer, stablecoin, etc.).

---

## Setting a revenue share policy

```sh
# Allow remote.example to earn 5% on referred purchases
curl -X POST https://ppv.example.com/api/federation/admin/revenue/policies \
  -H "X-Federation-Admin-Token: $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"domain":"remote.example","share_basis_points":500}'
```

`share_basis_points` is in the range \[0, 5000\]:
* 100 bp = 1 %
* 500 bp = 5 %
* 5000 bp = 50 % (maximum allowed)

---

## How shares are calculated

```
share_cents = floor(gross_cents * basis_points / 10_000)
```

Example: $12.99 sale (1299 cents) at 500 bp:
```
floor(1299 × 500 / 10 000) = floor(64.95) = 64 cents → $0.64
```

The remainder (35 cents) stays with this instance.  No floating-point
arithmetic is used; all calculations are in integer minor units.

---

## Viewing the provider report

```sh
curl https://ppv.example.com/api/federation/admin/revenue/provider-report \
  -H "X-Federation-Admin-Token: $TOKEN" | jq .
```

Response fields per domain:
* `pending_cents` — owed but not yet paid out
* `settled_cents` — already paid out
* `reversed_cents` — refunded/charged back
* `payment_count` — number of qualifying purchases

---

## Marking shares as settled

After paying a provider, update the share status directly in the database:

```sql
-- Mark all pending shares from remote.example as settled
UPDATE federation_revenue_shares
SET status = 'settled', updated_at = NOW()
WHERE referring_domain = 'remote.example'
  AND status = 'pending';

-- Append a debit ledger entry for audit
INSERT INTO revenue_ledger_entries
    (id, revenue_share_id, entry_type, amount_cents, description)
SELECT gen_random_uuid(), id, 'debit', share_cents, 'Wire transfer 2026-06'
FROM federation_revenue_shares
WHERE referring_domain = 'remote.example'
  AND status = 'settled'
  AND updated_at >= NOW() - INTERVAL '1 minute';
```

A future release will expose a `POST /api/federation/admin/revenue/settle`
endpoint to automate this.

---

## Refund and chargeback handling

When a payment is refunded or charged back, call `reverse_revenue_share`:

```rust
federation::revenue::reverse_revenue_share(
    &pool,
    &invoice_id,
    "x402",   // or "fiat"
    "refund", // or "chargeback"
).await?;
```

This sets the share status to `reversed` and appends a reversal ledger
entry.  The referring instance is not automatically notified; settlement
amounts can be reconciled against the ledger.

---

## X402 direct on-chain split (optional)

For X402 payments, the frontend can optionally request a 3-way smart
contract split that credits the traffic provider on-chain, eliminating
off-chain accounting for that payment:

```sh
GET /api/federation/referral/resolve?token=<fed_ref>&actor_url=<actor_url>
```

Response when direct split is possible:
```json
{
  "ok": true,
  "referring_domain": "remote.example",
  "split_enabled": true,
  "share_basis_points": 500,
  "provider_wallet": null
}
```

`provider_wallet` is `null` until the referring instance registers a
wallet address on their `federation_instances` row.  When `split_enabled`
is false, use off-chain accounting via `process_revenue_share` instead.
