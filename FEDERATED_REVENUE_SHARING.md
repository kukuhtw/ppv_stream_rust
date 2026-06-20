# Federated Revenue Sharing and Affiliate Commission Model

## 1. Status

This document defines the approved revenue-sharing model for federated PPV Stream providers.

The approved model is a **hybrid settlement model**.

Blockchain wallets are optional for traffic providers and affiliate users. The origin provider remains responsible for calculating revenue shares, recording liabilities, and paying beneficiaries through their selected payout method.

## 2. Architecture Boundary

Federation remains index-only.

Remote PPV Stream providers may display public creator and video metadata, but they do not receive or store:

- Original video files
- MP4 files
- HLS manifests
- HLS segments
- Transcoded media
- Playback sessions
- Watermark output
- Protected media URLs

The origin provider remains authoritative for:

- Video hosting
- Price
- Checkout
- Payment confirmation
- Playback authorization
- Streaming
- Refunds
- Chargebacks
- Revenue calculation
- Settlement accounting

## 3. Main Actors

### 3.1 Buyer

The buyer discovers a video, completes payment on the origin provider, and receives playback access from the origin provider.

### 3.2 Creator

The creator owns the video and receives the primary revenue share.

### 3.3 Origin Provider

The origin provider hosts the video and processes the sale.

Responsibilities include:

- Storage
- Transcoding
- HLS streaming
- Watermarking
- Payment processing
- Access control
- Refunds
- Chargebacks
- Revenue ledger
- Provider settlement
- Affiliate settlement

### 3.4 Traffic Provider

The traffic provider is another PPV Stream instance that displays the remote video index and sends a buyer to the origin provider.

It earns a federation referral fee when its signed referral results in a confirmed sale.

### 3.5 Affiliate User

The affiliate user promotes another creator's video through a referral link.

The affiliate may belong to:

- The origin provider
- The traffic provider
- Another trusted provider

Affiliate commission is separate from the traffic provider fee.

### 3.6 Payment Provider

Examples:

- Stripe
- PayPal
- Midtrans
- Xendit
- Internal wallet
- X402 blockchain payment

Payment fees should normally be deducted before distributable revenue is calculated.

## 4. Approved Hybrid Settlement Model

The hybrid model combines on-chain payment with off-chain accounting.

### 4.1 Core Rule

When all beneficiaries have compatible blockchain wallets, the system may pay them directly on-chain.

When one or more beneficiaries do not have blockchain wallets, the smart contract sends their combined share to the origin provider settlement wallet.

The origin provider then records each beneficiary's share as a payable liability.

### 4.2 Default X402 Flow

```text
Buyer pays through X402
    -> Smart contract pays creator final share
    -> Smart contract pays all non-creator funds to origin settlement wallet
    -> Origin provider creates immutable ledger entries
    -> Origin provider retains its own fee
    -> Traffic provider amount becomes payable
    -> Affiliate amount becomes payable
    -> Payout occurs through the beneficiary's selected method
```

### 4.3 Why This Model Is Approved

This model:

- Does not require every provider to understand blockchain
- Does not require every affiliate to own a wallet
- Supports bank and gateway payouts
- Preserves blockchain payment for buyers
- Supports refund and chargeback accounting
- Allows payout thresholds
- Reduces gas costs
- Allows weekly or monthly settlement
- Keeps revenue policies configurable

## 5. Revenue Components

```text
Gross Sale Amount
    minus Payment Processing Fee
    minus Tax or Mandatory Charges
    equals Net Distributable Revenue
```

Net distributable revenue may be allocated to:

- Creator
- Origin provider
- Traffic provider
- Affiliate user
- Optional protocol treasury

## 6. Recommended Default Percentages

The default policy is configurable, but the recommended starting point is:

```text
Creator base share:            80%
Platform revenue pool:         20%
```

### 6.1 Direct Purchase

```text
Creator:                        80%
Origin provider:                20%
Traffic provider:                0%
Affiliate:                       0%
```

### 6.2 Federated Purchase Without Affiliate

```text
Creator:                        80%
Origin provider:                12%
Traffic provider:                8%
Affiliate:                       0%
```

### 6.3 Federated Purchase With Affiliate

Affiliate commission is deducted from the creator base share by default.

Example with 10% affiliate commission:

```text
Creator base share:            80%
Affiliate commission:          10%
Creator final share:           70%
Origin provider:               12%
Traffic provider:               8%
```

Total:

```text
70% + 10% + 12% + 8% = 100%
```

## 7. Example Calculation

```text
Video price:                   USD 20.00
Payment fee:                   USD  1.00
Net distributable revenue:     USD 19.00
```

Policy:

```text
Creator base:                  80%
Origin provider:               12%
Traffic provider:               8%
Affiliate:                     10% from creator share
```

Calculation:

```text
Creator base:          19.00 x 80% = USD 15.20
Affiliate:             19.00 x 10% = USD  1.90
Creator final:         15.20 - 1.90 = USD 13.30
Origin provider:       19.00 x 12% = USD  2.28
Traffic provider:      19.00 x  8% = USD  1.52
```

Final distribution:

| Recipient | Amount |
|---|---:|
| Creator | USD 13.30 |
| Origin provider | USD 2.28 |
| Traffic provider | USD 1.52 |
| Affiliate user | USD 1.90 |
| Total | USD 19.00 |

## 8. X402 Settlement Without Beneficiary Wallets

Assume the traffic provider and affiliate do not have blockchain wallets.

The smart contract should distribute:

```text
Creator wallet:                70%
Origin settlement wallet:      30%
```

The backend then records the 30% as:

```text
Origin provider revenue:       12%
Traffic provider payable:       8%
Affiliate payable:             10%
```

Example using 100 USDC:

```text
Smart contract transfer
    Creator wallet:            70 USDC
    Origin settlement wallet:  30 USDC

Origin provider ledger
    Origin provider revenue:   12 USDC
    Traffic provider payable:   8 USDC
    Affiliate payable:         10 USDC
```

The origin provider must not treat the full 30 USDC as income.

The 18 USDC owed to the traffic provider and affiliate is a liability.

## 9. Beneficiary Payout Methods

Blockchain wallet is optional.

A traffic provider or affiliate may select:

- Internal PPV wallet
- Bank transfer
- PayPal
- Xendit disbursement
- Stripe Connect
- Stablecoin wallet
- Manual settlement

The selected payout method belongs in a payout profile.

## 10. Payout Profiles

Recommended table:

```sql
CREATE TABLE payout_profiles (
    id UUID PRIMARY KEY,
    owner_type TEXT NOT NULL,
    owner_reference TEXT NOT NULL,
    payout_method TEXT NOT NULL,
    payout_currency TEXT NOT NULL,
    bank_account_encrypted JSONB,
    paypal_email TEXT,
    blockchain_address TEXT,
    blockchain_network TEXT,
    minimum_payout_minor BIGINT NOT NULL DEFAULT 0,
    verification_status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (owner_type, owner_reference)
);
```

Owner types:

- provider
- affiliate
- creator

Payout methods:

- internal_wallet
- bank_transfer
- paypal
- xendit
- stripe_connect
- blockchain
- manual

Sensitive payout data must be encrypted at rest.

## 11. Signed Referral Attribution

The traffic provider and affiliate must be identified before invoice creation.

Example signed payload:

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

- Traffic provider identity
- Signature
- Expiration
- Video binding
- Affiliate identity
- Referral uniqueness
- Domain moderation status

Unsigned query parameters must never determine revenue attribution.

## 12. Invoice Capture Rule

Attribution must be captured when the invoice is created.

Recommended invoice fields:

```text
traffic_instance
traffic_referral_id
affiliate_actor_uri
affiliate_referral_id
revenue_share_policy_id
revenue_share_snapshot
settlement_mode
```

Example policy snapshot:

```json
{
  "creator_base_bps": 8000,
  "creator_final_bps": 7000,
  "origin_provider_bps": 1200,
  "traffic_provider_bps": 800,
  "affiliate_bps": 1000,
  "affiliate_funding_source": "creator_share",
  "settlement_mode": "hybrid",
  "traffic_provider_payout_method": "bank_transfer",
  "affiliate_payout_method": "internal_wallet"
}
```

The payment result must use this snapshot even if policy settings change later.

## 13. Calculation Order

1. Confirm gross amount.
2. Deduct payment processing fee.
3. Deduct tax or mandatory charges.
4. Calculate net distributable revenue.
5. Calculate creator base share.
6. Calculate origin provider share.
7. Calculate traffic provider share.
8. Calculate affiliate commission.
9. Deduct affiliate commission from its configured funding source.
10. Determine direct on-chain recipients.
11. Send all non-direct shares to the origin settlement wallet.
12. Create immutable revenue ledger entries.
13. Grant buyer access.
14. Schedule beneficiary payouts.

## 14. Minor Units and Basis Points

All calculations must use integer minor units.

Examples:

- USD cents
- IDR rupiah
- USDC base units

Do not use floating-point arithmetic.

Formula:

```text
share_minor = net_amount_minor x basis_points / 10,000
```

Recommended rounding policy:

- Calculate each share using integer division.
- Assign the final remainder to the origin provider.
- Store the remainder explicitly for audit.

## 15. Revenue Ledger

Recommended table:

```sql
CREATE TABLE federation_revenue_shares (
    id UUID PRIMARY KEY,
    invoice_uid TEXT NOT NULL UNIQUE,
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
    settlement_wallet_amount_minor BIGINT NOT NULL DEFAULT 0,
    currency TEXT NOT NULL,
    settlement_mode TEXT NOT NULL DEFAULT 'hybrid',
    status TEXT NOT NULL DEFAULT 'confirmed',
    policy_snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    settled_at TIMESTAMPTZ,
    reversed_at TIMESTAMPTZ
);
```

Recommended ledger entries:

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
    payout_profile_id UUID,
    external_reference TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    paid_at TIMESTAMPTZ,
    reversed_at TIMESTAMPTZ
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

## 16. Ledger Statuses

Recommended statuses:

- confirmed
- payable
- on_hold
- scheduled
- processing
- paid
- failed
- reversed
- disputed

A successful blockchain payment does not automatically mean every beneficiary has been paid.

Example:

```text
Creator entry:           paid
Origin provider entry:   confirmed
Traffic provider entry:  payable
Affiliate entry:         payable
```

## 17. Settlement Schedule

The origin provider may settle beneficiaries:

- Daily
- Weekly
- Monthly
- After a minimum payout threshold

Example:

```text
Traffic provider monthly earnings: USD 80
Refund adjustment:                 USD 10
Chargeback adjustment:             USD  5
Final payable:                      USD 65
```

## 18. Remote Affiliate Settlement

When the affiliate belongs to another provider, two settlement approaches are allowed.

### 18.1 Direct Affiliate Payout

The origin provider pays the affiliate using the affiliate's verified payout profile.

### 18.2 Provider-Mediated Payout

The origin provider pays the affiliate's home provider.

The home provider then credits the affiliate's account.

Provider-mediated payout is recommended when:

- The affiliate uses an internal wallet
- The home provider handles KYC
- The origin provider does not support the affiliate's local payment method

## 19. Smart Contract Decision

### 19.1 Existing Contract

The existing two-recipient contract can support the hybrid MVP.

For a federated sale with an affiliate:

```text
creatorBp = creator final percentage
admin receives = all non-creator percentages
```

Example:

```text
Creator final:                  7000 bps
Origin settlement wallet:      3000 bps
```

Backend allocation of the settlement amount:

```text
Origin provider:               1200 bps
Traffic provider:               800 bps
Affiliate:                     1000 bps
```

### 19.2 Recommended New Contract

A new contract may be introduced as:

```text
X402FederatedSplitter.sol
```

It should not require four blockchain wallets.

The recommended contract supports:

- Creator direct recipient
- Origin settlement wallet
- Optional direct traffic-provider recipient
- Optional direct affiliate recipient
- Signed beneficiary identifiers
- Signed policy snapshot hash

### 19.3 Optional Direct Payment

If a beneficiary has a verified blockchain wallet, its share may be paid directly.

If not, its share is included in the origin settlement wallet allocation.

Example:

```text
Creator has wallet:             direct
Traffic provider has wallet:    direct
Affiliate has no wallet:        settlement wallet
Origin provider:                settlement wallet
```

The backend remains authoritative for the off-chain ledger.

## 20. Smart Contract Metadata

The signed invoice should bind:

- Invoice UID
- Token
- Minimum amount
- Creator address
- Creator final basis points
- Origin settlement wallet
- Direct recipient addresses, when present
- Direct recipient basis points
- Traffic provider beneficiary ID
- Affiliate beneficiary ID
- Revenue policy snapshot hash
- Video ID hash
- Payer
- Deadline
- Contract address
- Chain ID

A beneficiary ID may be:

```text
keccak256(provider actor URI)
keccak256(affiliate actor URI)
```

This records attribution without requiring a wallet address.

## 21. Suggested Smart Contract Event

```solidity
event FederatedPaid(
    bytes32 indexed invoiceUid,
    address indexed payer,
    address indexed creator,
    address settlementWallet,
    address token,
    uint256 totalAmount,
    uint256 creatorAmount,
    uint256 settlementAmount,
    bytes32 trafficProviderId,
    bytes32 affiliateId,
    bytes32 policyHash,
    string videoId
);
```

The event records beneficiary identities without requiring their blockchain wallets.

## 22. Refund and Chargeback Handling

Refunds and chargebacks must reverse all related allocations.

Required behavior:

1. Preserve the original entries.
2. Create negative reversal entries.
3. Reduce unsettled payable balances.
4. Create beneficiary debt if already paid.
5. Revoke playback access when required.
6. Mark the revenue share as reversed or disputed.

Never delete financial ledger records.

## 23. Fraud Prevention

Required controls:

- Signed traffic referrals
- Signed remote affiliate attribution
- Referral expiration
- One traffic provider per invoice
- One affiliate per invoice
- Idempotent revenue processing
- Self-referral prevention
- Creator cannot affiliate their own video
- Buyer cannot be the affiliate
- Blocked providers cannot earn new fees
- Webhook replay protection
- Referral and video binding
- Invoice-time policy snapshot
- Verified payout profiles
- Payout approval controls
- Reconciliation reports

## 24. Accounting Rule

Amounts owed to traffic providers and affiliates must be recorded as liabilities.

They must not be recognized as origin-provider revenue.

Example:

```text
Settlement wallet balance:     30 USDC
Origin provider revenue:       12 USDC
Traffic provider payable:       8 USDC
Affiliate payable:             10 USDC
```

The accounting system must preserve this separation.

## 25. Recommended Environment Configuration

```env
FEDERATION_CREATOR_BASE_BPS=8000
FEDERATION_ORIGIN_PROVIDER_BPS=1200
FEDERATION_TRAFFIC_PROVIDER_BPS=800
AFFILIATE_MAX_BPS=3000
AFFILIATE_FUNDING_SOURCE=creator_share
FEDERATION_SETTLEMENT_MODE=hybrid
FEDERATION_SETTLEMENT_WALLET=0x...
FEDERATION_MIN_PAYOUT_MINOR=5000
FEDERATION_SETTLEMENT_CYCLE=monthly
```

## 26. Recommended API Endpoints

```http
POST /api/federation/referrals
POST /api/federation/purchase/start
POST /api/federation/revenue/confirm
GET  /api/federation/provider-earnings
GET  /api/federation/affiliate-earnings
GET  /api/payout-profile
POST /api/payout-profile
GET  /admin/federation/settlements
POST /admin/federation/settlements
POST /admin/federation/settlements/:id/complete
POST /admin/federation/settlements/:id/retry
```

## 27. Implementation Order

1. Add payout profiles.
2. Add hybrid settlement configuration.
3. Add signed referral attribution.
4. Capture attribution and policy snapshot at invoice creation.
5. Add revenue share and ledger tables.
6. Calculate shares in integer minor units.
7. Route creator final share on-chain.
8. Route non-direct shares to the settlement wallet.
9. Create provider and affiliate payable entries.
10. Add settlement dashboard.
11. Add refund and chargeback reversal.
12. Add optional direct blockchain recipients.
13. Add reconciliation reports.
14. Introduce `X402FederatedSplitter.sol` only after audit and testing.

## 28. Final Approved Rule

> PPV Stream uses hybrid federated settlement. Blockchain wallets are optional for traffic providers and affiliates. The creator may receive a direct on-chain payment, while all non-direct beneficiary shares are sent to the origin provider settlement wallet and recorded as payable liabilities in an immutable ledger.

This model supports decentralized traffic distribution without forcing every provider or affiliate to adopt blockchain infrastructure.
