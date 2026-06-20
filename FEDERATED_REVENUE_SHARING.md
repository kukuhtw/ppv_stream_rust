# Federated Revenue Sharing and Affiliate Commission Model

## 1. Purpose

This document defines how revenue should be shared when a video sale involves multiple parties across a federated PPV Stream network.

The model covers two main scenarios:

1. A remote PPV Stream provider drives traffic to a video hosted by another PPV Stream provider.
2. A user also promotes that video through an affiliate referral link.

The architecture remains index-only federation:

- Remote providers display public video metadata and canonical links.
- The origin provider stores and streams the video.
- The origin provider processes the payment.
- Revenue sharing is calculated by the origin provider after payment confirmation.
- Remote providers do not receive or store the actual video content.

## 2. Main Actors

### 2.1 Buyer

The buyer discovers a video, completes payment, and receives playback access from the origin provider.

The buyer pays the same published price regardless of whether the purchase came from:

- Direct traffic
- A federated provider
- An affiliate user
- A federated provider plus an affiliate user

### 2.2 Creator

The creator owns the video and defines its sale price.

The creator receives the primary revenue share after deductions for:

- Payment processing fees
- Origin provider fee
- Traffic provider fee
- Affiliate commission
- Applicable tax or refund adjustments

### 2.3 Origin Provider

The origin provider is the PPV Stream instance where the video is hosted.

The origin provider is responsible for:

- Video storage
- Upload processing
- Transcoding
- HLS generation
- Watermarking
- Payment processing
- Playback authorization
- Purchase records
- Refunds
- Chargebacks
- Customer support
- Settlement accounting

The origin provider is the authoritative system for the sale.

### 2.4 Traffic Provider

The traffic provider is another PPV Stream instance that displays the remote video index and sends the buyer to the origin provider.

The traffic provider earns a federation referral fee when it successfully drives a paid conversion.

The traffic provider does not:

- Store the video
- Process playback
- Issue access rights
- Confirm the payment
- Override the origin price

### 2.5 Affiliate User

The affiliate user is an individual account that promotes another user's video through a unique referral link.

The affiliate user may belong to:

- The origin provider
- The traffic provider
- Another trusted federated provider

The affiliate commission is separate from the traffic provider fee.

### 2.6 Payment Provider

The payment provider may be:

- Stripe
- PayPal
- Midtrans
- Xendit
- Internal wallet
- X402 blockchain payment

Payment processing fees should normally be deducted before distributable revenue is calculated.

## 3. Revenue Components

Every successful sale may contain the following components:

```text
Gross Sale Amount
    minus Payment Processing Fee
    minus Tax or Mandatory Charges
    equals Net Distributable Revenue
```

The net distributable revenue may then be divided among:

- Creator
- Origin provider
- Traffic provider
- Affiliate user
- Optional protocol treasury

## 4. Recommended Revenue Sharing Principle

The recommended model is:

1. Payment fees are deducted first.
2. Creator share is calculated from net distributable revenue.
3. Platform revenue is divided between the origin provider and traffic provider.
4. Affiliate commission is deducted according to the creator's affiliate settings or a configured promotional pool.
5. All amounts are recorded in an immutable revenue ledger.

This prevents hidden payment costs and makes every share auditable.

## 5. Scenario A: Direct Purchase Without Federation or Affiliate

Example:

```text
Gross sale amount:             USD 10.00
Payment processing fee:        USD  0.50
Net distributable revenue:     USD  9.50
```

Recommended split:

| Recipient | Percentage of Net Revenue | Amount |
|---|---:|---:|
| Creator | 80% | USD 7.60 |
| Origin provider | 20% | USD 1.90 |
| Total | 100% | USD 9.50 |

There is no traffic provider fee and no affiliate commission.

## 6. Scenario B: Purchase Driven by Another Federated Provider

Example:

```text
Provider A = Origin provider
Provider B = Traffic provider
Creator = User hosted on Provider A
Buyer = User who discovers the video on Provider B
```

Flow:

```text
Buyer browses Provider B
    -> Provider B displays remote video metadata from Provider A
    -> Buyer clicks the canonical video link
    -> Buyer is redirected to Provider A
    -> Provider A creates the invoice
    -> Provider A confirms payment
    -> Provider A grants playback access
    -> Provider A records revenue shares
```

Example calculation:

```text
Gross sale amount:             USD 10.00
Payment processing fee:        USD  0.50
Net distributable revenue:     USD  9.50
```

Recommended split:

| Recipient | Percentage of Net Revenue | Amount |
|---|---:|---:|
| Creator | 80% | USD 7.60 |
| Origin provider | 12% | USD 1.14 |
| Traffic provider | 8% | USD 0.76 |
| Total | 100% | USD 9.50 |

This model preserves the creator's normal 80% share while the origin provider shares part of its platform fee with the traffic provider.

## 7. Scenario C: Purchase Through an Affiliate Link Without Federation

Example:

```text
Creator and affiliate are registered on the same provider.
Buyer clicks the affiliate link directly.
```

The affiliate user earns a commission configured by the creator for that video.

Example:

```text
Gross sale amount:             USD 10.00
Payment processing fee:        USD  0.50
Net distributable revenue:     USD  9.50
Creator base share:            80%
Affiliate commission:          10% of net distributable revenue
```

Recommended split:

| Recipient | Amount |
|---|---:|
| Creator before affiliate | USD 7.60 |
| Affiliate commission | USD 0.95 |
| Creator after affiliate | USD 6.65 |
| Origin provider | USD 1.90 |
| Total | USD 9.50 |

In this model, affiliate commission comes from the creator's share.

This is consistent with the current PPV Stream affiliate concept where the creator decides the commission percentage and funds the affiliate reward.

## 8. Scenario D: Federated Traffic Provider Plus Affiliate User

This is the most complete scenario.

Example:

```text
Provider A = Origin provider
Provider B = Traffic provider
Creator = User on Provider A
Affiliate = User on Provider B
Buyer = User who discovers the video through Provider B and the affiliate link
```

Flow:

```text
Affiliate user shares a federated referral link
    -> Buyer opens the link on Provider B
    -> Provider B records the affiliate attribution
    -> Provider B redirects the buyer to Provider A
    -> Provider A receives signed traffic-provider and affiliate attribution
    -> Provider A creates the invoice
    -> Payment is confirmed
    -> Provider A calculates all revenue shares
```

Recommended calculation:

```text
Gross sale amount:             USD 10.00
Payment processing fee:        USD  0.50
Net distributable revenue:     USD  9.50
```

Recommended split:

| Recipient | Percentage of Net Revenue | Amount |
|---|---:|---:|
| Creator base share | 80% | USD 7.60 |
| Origin provider | 12% | USD 1.14 |
| Traffic provider | 8% | USD 0.76 |
| Total before affiliate | 100% | USD 9.50 |

If the creator has configured an affiliate commission of 10% of net distributable revenue:

```text
Affiliate commission:          USD 0.95
Creator final amount:          USD 6.65
Origin provider amount:        USD 1.14
Traffic provider amount:       USD 0.76
Affiliate amount:              USD 0.95
```

Final distribution:

| Recipient | Final Amount |
|---|---:|
| Creator | USD 6.65 |
| Origin provider | USD 1.14 |
| Traffic provider | USD 0.76 |
| Affiliate user | USD 0.95 |
| Total | USD 9.50 |

## 9. Alternative Affiliate Funding Models

### 9.1 Creator-Funded Affiliate Commission

This is the recommended default.

```text
Creator base share
    minus Affiliate commission
    equals Creator final share
```

Advantages:

- Creator controls promotion cost
- Platform fee remains predictable
- Traffic provider fee remains predictable
- Existing affiliate behavior can be reused

Disadvantages:

- High affiliate percentages can significantly reduce creator revenue

### 9.2 Shared Promotional Pool

The affiliate commission may be funded proportionally by multiple parties.

Example:

```text
Affiliate commission: USD 0.95
Creator funds:         USD 0.57
Origin provider funds: USD 0.23
Traffic provider funds: USD 0.15
```

Advantages:

- Promotion cost is shared
- Creator retains more revenue
- Providers participate in growth incentives

Disadvantages:

- More complex accounting
- More complex contracts between providers
- More difficult dispute handling

### 9.3 Platform-Funded Affiliate Commission

The affiliate commission may come entirely from the origin provider fee.

Advantages:

- Creator revenue remains unchanged
- Attractive for creators

Disadvantages:

- Origin provider margin becomes smaller
- Unsustainable if affiliate percentage is too high

## 10. Recommended Default Policy

The recommended default is:

```text
Creator share:                  80% of net distributable revenue
Platform revenue pool:          20% of net distributable revenue
```

When the sale is direct:

```text
Creator:                        80%
Origin provider:                20%
```

When a federated provider drives the sale:

```text
Creator:                        80%
Origin provider:                12%
Traffic provider:                8%
```

When an affiliate also participates:

```text
Affiliate commission is deducted from the creator's 80% share
```

Example with a 10% affiliate commission:

```text
Creator base:                   80%
Affiliate:                      10%
Creator final:                  70%
Origin provider:                12%
Traffic provider:                8%
```

The percentages above are examples and should be configurable.

## 11. Attribution Data

The traffic provider and affiliate must be identified before the invoice is created.

Recommended referral payload:

```json
{
  "video_object_uri": "https://provider-a.example/federation/videos/video-123",
  "origin_instance": "provider-a.example",
  "traffic_instance": "provider-b.example",
  "affiliate_actor_uri": "https://provider-b.example/users/alice",
  "referral_id": "REF-20260620-000123",
  "issued_at": "2026-06-20T10:00:00Z",
  "expires_at": "2026-06-20T11:00:00Z",
  "signature": "signed-by-provider-b"
}
```

The origin provider must verify:

- The traffic provider is known and trusted
- The signature is valid
- The referral has not expired
- The referral is linked to the correct video
- The affiliate actor exists
- The referral has not been reused improperly
- The traffic provider is not blocked

## 12. Signed Referral URL

Example:

```text
https://provider-a.example/federation/checkout/video-123
?traffic_instance=provider-b.example
&affiliate_actor=https%3A%2F%2Fprovider-b.example%2Fusers%2Falice
&referral_id=REF-20260620-000123
&expires=1781953200
&signature=abc123
```

The origin provider must never trust unsigned query parameters for revenue attribution.

## 13. Invoice Capture Rule

Attribution must be stored when the invoice is created.

Recommended invoice fields:

```text
traffic_instance
traffic_referral_id
affiliate_actor_uri
affiliate_referral_id
revenue_share_policy_id
revenue_share_snapshot
```

The revenue share snapshot is important because percentages may change later.

Example snapshot:

```json
{
  "creator_pct": 80,
  "origin_provider_pct": 12,
  "traffic_provider_pct": 8,
  "affiliate_pct": 10,
  "affiliate_funding_source": "creator_share"
}
```

The payment result must use the snapshot stored at invoice creation, not the current configuration.

## 14. Revenue Calculation Order

The recommended calculation order is:

1. Confirm gross sale amount.
2. Deduct payment processing fee.
3. Deduct tax or mandatory charges.
4. Calculate net distributable revenue.
5. Calculate creator base share.
6. Calculate origin provider share.
7. Calculate traffic provider share.
8. Calculate affiliate commission.
9. Deduct affiliate commission from the selected funding source.
10. Store immutable revenue ledger entries.
11. Grant buyer access.
12. Schedule provider settlement.

## 15. Calculation Formula

```text
Net Revenue = Gross Amount - Payment Fee - Tax

Creator Base = Net Revenue × Creator Percentage
Origin Provider = Net Revenue × Origin Provider Percentage
Traffic Provider = Net Revenue × Traffic Provider Percentage
Affiliate Commission = Net Revenue × Affiliate Percentage

Creator Final = Creator Base - Affiliate Commission
```

Validation rule:

```text
Creator Final + Origin Provider + Traffic Provider + Affiliate Commission = Net Revenue
```

## 16. Rounding Rules

All monetary calculations should use integer minor units.

Examples:

- USD cents
- IDR rupiah
- Token base units

Do not use floating-point arithmetic.

Recommended method:

```text
amount_minor × basis_points / 10,000
```

Any rounding remainder should be assigned according to a documented policy.

Recommended default:

- Assign the remainder to the origin provider
- Record the remainder explicitly in the ledger

## 17. Basis Point Configuration

Use basis points for precise configuration.

Example:

```text
Creator:                 8,000 bps
Origin provider:         1,200 bps
Traffic provider:          800 bps
Total:                  10,000 bps
```

Affiliate commission may use a separate basis-point field:

```text
Affiliate commission:    1,000 bps
```

If affiliate commission is creator-funded, it is deducted from the creator allocation after the base split.

## 18. Recommended Database Tables

### 18.1 federation_referrals

```sql
CREATE TABLE federation_referrals (
    id UUID PRIMARY KEY,
    referral_id TEXT NOT NULL UNIQUE,
    video_object_uri TEXT NOT NULL,
    origin_instance TEXT NOT NULL,
    traffic_instance TEXT NOT NULL,
    affiliate_actor_uri TEXT,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    signature TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 18.2 revenue_share_policies

```sql
CREATE TABLE revenue_share_policies (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    creator_bps INT NOT NULL,
    origin_provider_bps INT NOT NULL,
    traffic_provider_bps INT NOT NULL,
    affiliate_funding_source TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (creator_bps + origin_provider_bps + traffic_provider_bps = 10000)
);
```

### 18.3 federation_revenue_shares

```sql
CREATE TABLE federation_revenue_shares (
    id UUID PRIMARY KEY,
    invoice_uid TEXT NOT NULL,
    video_id TEXT NOT NULL,
    buyer_id TEXT,
    buyer_actor_uri TEXT,
    origin_instance TEXT NOT NULL,
    traffic_instance TEXT,
    affiliate_actor_uri TEXT,
    gross_amount_minor BIGINT NOT NULL,
    payment_fee_minor BIGINT NOT NULL,
    tax_amount_minor BIGINT NOT NULL DEFAULT 0,
    net_amount_minor BIGINT NOT NULL,
    creator_amount_minor BIGINT NOT NULL,
    origin_provider_amount_minor BIGINT NOT NULL,
    traffic_provider_amount_minor BIGINT NOT NULL DEFAULT 0,
    affiliate_amount_minor BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'confirmed',
    policy_snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    settled_at TIMESTAMPTZ,
    reversed_at TIMESTAMPTZ,
    UNIQUE (invoice_uid)
);
```

### 18.4 revenue_ledger_entries

```sql
CREATE TABLE revenue_ledger_entries (
    id UUID PRIMARY KEY,
    revenue_share_id UUID NOT NULL REFERENCES federation_revenue_shares(id),
    recipient_type TEXT NOT NULL,
    recipient_reference TEXT NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    entry_type TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

Recipient types:

- creator
- origin_provider
- traffic_provider
- affiliate
- payment_provider
- tax_authority
- protocol_treasury

## 19. Settlement Model

### 19.1 Internal Ledger Settlement

For the first implementation, the origin provider should record payable amounts in a ledger.

The traffic provider and remote affiliate are paid later.

Recommended settlement schedules:

- Daily
- Weekly
- Monthly
- After a minimum payout threshold

### 19.2 Provider Settlement Example

```text
Traffic provider: Provider B
Monthly eligible sales: USD 1,000
Traffic provider share: USD 80
Refund adjustment: USD 10
Chargeback adjustment: USD 5
Final payable: USD 65
```

Settlement methods may include:

- Bank transfer
- PayPal payout
- Stripe Connect
- Xendit disbursement
- Stablecoin transfer
- X402 settlement

### 19.3 Affiliate Settlement

Affiliate earnings may be settled through:

- Internal wallet if the affiliate is local
- Provider-to-provider settlement if the affiliate is remote
- Direct blockchain transfer
- External payout account

For remote affiliates, the origin provider may settle to the affiliate's home provider, which then credits the affiliate account.

## 20. X402 Smart Contract Split

For on-chain payments, revenue may be split immediately.

Example:

```text
Buyer pays:               10 USDC
Creator receives:          7.00 USDC
Origin provider receives:  1.20 USDC
Traffic provider receives: 0.80 USDC
Affiliate receives:        1.00 USDC
```

Example basis points:

```text
Creator final:             7,000 bps
Origin provider:           1,200 bps
Traffic provider:            800 bps
Affiliate:                 1,000 bps
Total:                    10,000 bps
```

The contract should validate that the total equals 10,000 basis points.

## 21. Refund and Chargeback Handling

A refund or chargeback must reverse all related revenue shares.

Recommended behavior:

1. Mark the original revenue share as reversed.
2. Create negative ledger entries for every recipient.
3. Reduce unsettled payable balances.
4. If already settled, create a debt balance for the next settlement cycle.
5. Revoke playback access when policy requires it.
6. Preserve the original audit records.

Never delete the original ledger entries.

## 22. Fraud Prevention Rules

The implementation should enforce:

- Signed traffic-provider referrals
- Signed affiliate attribution for remote affiliates
- Referral expiration
- One traffic provider per invoice
- One affiliate per invoice
- Idempotent commission processing
- Self-referral prevention
- Creator cannot affiliate their own video
- Buyer cannot be the affiliate
- Blocked providers cannot earn new fees
- Replayed webhooks cannot create duplicate shares
- Referral must match the purchased video
- Referral must be captured before payment confirmation
- All percentage settings must be snapshotted

## 23. Self-Referral Rules

Reject commission when:

- Buyer actor equals affiliate actor
- Creator actor equals affiliate actor
- Traffic provider and origin provider are the same instance and local traffic rules do not allow a provider fee
- The same operator controls both instances and policy prohibits related-party referral fees

## 24. Configuration Example

```env
FEDERATION_CREATOR_BPS=8000
FEDERATION_ORIGIN_PROVIDER_BPS=1200
FEDERATION_TRAFFIC_PROVIDER_BPS=800
AFFILIATE_MAX_BPS=3000
AFFILIATE_FUNDING_SOURCE=creator_share
FEDERATION_MIN_PAYOUT_MINOR=5000
FEDERATION_SETTLEMENT_CYCLE=monthly
```

## 25. API Suggestions

### Create Federated Referral

```http
POST /api/federation/referrals
```

### Start Purchase

```http
POST /api/federation/purchase/start
```

### Confirm Revenue Share

```http
POST /api/federation/revenue/confirm
```

### List Provider Earnings

```http
GET /api/federation/provider-earnings
```

### List Affiliate Earnings

```http
GET /api/federation/affiliate-earnings
```

### Admin Settlement

```http
POST /admin/federation/settlements
GET /admin/federation/settlements
POST /admin/federation/settlements/:id/complete
```

## 26. End-to-End Example

Assume:

```text
Video price:                   USD 20.00
Payment fee:                   USD  1.00
Net distributable revenue:     USD 19.00
Creator base share:            80%
Origin provider share:         12%
Traffic provider share:         8%
Affiliate commission:          10% of net revenue
Affiliate funding source:      Creator share
```

Calculation:

```text
Creator base:          19.00 × 80% = USD 15.20
Origin provider:       19.00 × 12% = USD  2.28
Traffic provider:      19.00 ×  8% = USD  1.52
Affiliate commission:  19.00 × 10% = USD  1.90
Creator final:         15.20 - 1.90 = USD 13.30
```

Final distribution:

| Recipient | Amount |
|---|---:|
| Creator | USD 13.30 |
| Origin provider | USD 2.28 |
| Traffic provider | USD 1.52 |
| Affiliate user | USD 1.90 |
| Total | USD 19.00 |

## 27. Recommended Implementation Order

1. Add revenue share policy configuration.
2. Add signed federated referral payloads.
3. Capture traffic provider and affiliate attribution at invoice creation.
4. Add immutable revenue share records.
5. Add idempotent ledger processing.
6. Add refund and chargeback reversal.
7. Add provider settlement dashboard.
8. Add remote affiliate settlement.
9. Add X402 direct split support.
10. Add reconciliation reports.

## 28. Final Recommended Model

The recommended business rule is:

> The origin provider processes the sale and remains authoritative. The creator receives the primary revenue share. The traffic provider receives a federation referral fee for driving the buyer. The affiliate user receives a separate commission when a valid affiliate referral contributed to the sale.

Recommended default allocation:

```text
Creator base share:            80%
Origin provider share:         12%
Traffic provider share:         8%
Affiliate commission:          Configurable and deducted from creator share
```

This model keeps creator revenue transparent, rewards federated providers for distribution, rewards individual affiliates for promotion, and preserves a clear audit trail for every sale.
