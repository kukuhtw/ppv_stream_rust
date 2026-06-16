# Payment Plugin Architecture

→ [README.md](README.md) | [PAYMENT.md](PAYMENT.md) | [AFFILIATE.md](AFFILIATE.md) | [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md)

This repository has a fully implemented payment plugin system under:

```text
src/plugins/payment/
```

The goal is to make payment providers configurable without hardcoding all provider logic directly inside `src/handlers/pay.rs`. All five providers (x402, Stripe, PayPal, Midtrans, Xendit) are now fully implemented.

## Supported Provider Targets

```text
x402
paypal
stripe
midtrans
xendit
```

## Environment Configuration

Enable providers with:

```dotenv
PAYMENT_PLUGINS=x402,paypal,stripe,midtrans,xendit
PAYMENT_DEFAULT_PROVIDER=x402
```

Examples:

```dotenv
PAYMENT_PLUGINS=midtrans,xendit
PAYMENT_DEFAULT_PROVIDER=midtrans
```

```dotenv
PAYMENT_PLUGINS=paypal,stripe
PAYMENT_DEFAULT_PROVIDER=stripe
```

## Provider Environment Variables

### x402

```dotenv
X402_CONTRACT_ADDRESS=0x...
X402_RPC_HTTP=https://...
X402_ADMIN_PRIVKEY=0x...
X402_CHAIN_ID=80002
```

### PayPal

```dotenv
PAYPAL_ENV=sandbox
PAYPAL_CLIENT_ID=...
PAYPAL_CLIENT_SECRET=...
PAYPAL_WEBHOOK_ID=...
```

### Stripe

```dotenv
STRIPE_ENV=test
STRIPE_SECRET_KEY=sk_test_...
STRIPE_WEBHOOK_SECRET=whsec_...
```

### Midtrans

```dotenv
MIDTRANS_ENV=sandbox
MIDTRANS_SERVER_KEY=...
MIDTRANS_CLIENT_KEY=...
```

### Xendit

```dotenv
XENDIT_ENV=test
XENDIT_SECRET_KEY=...
XENDIT_WEBHOOK_TOKEN=...
```

## Active Generic Routes

Default provider routes:

```text
GET  /api/pay/providers
POST /api/pay/start
POST /api/pay/confirm
```

Provider-specific routes:

```text
POST /api/pay/:provider/start
POST /api/pay/:provider/confirm
POST /api/pay/:provider/webhook   ← receives payment notifications from the provider
```

Legacy x402 routes remain available:

```text
POST /api/pay/x402/start
POST /api/pay/x402/confirm
```

## Check Enabled Providers

```bash
curl http://localhost:8080/api/pay/providers
```

The response includes:

```text
configured
environment
api_base_url
required_env
missing_env
supported_currencies
```

## Create Invoice Through Default Provider

```bash
curl -X POST http://localhost:8080/api/pay/start \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "user-1",
    "video_id": "video-1",
    "amount_cents": 10000,
    "currency": "IDR",
    "buyer_email": "buyer@example.com",
    "buyer_name": "Demo Buyer"
  }'
```

## Create Invoice Through Explicit Provider

```bash
curl -X POST http://localhost:8080/api/pay/midtrans/start \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "user-1",
    "video_id": "video-1",
    "amount_cents": 10000,
    "currency": "IDR",
    "buyer_email": "buyer@example.com",
    "buyer_name": "Demo Buyer"
  }'
```

## Create x402 Invoice Through Generic Plugin Route

The generic x402 route now creates the same type of signed authorization payload as the legacy x402 start endpoint. It requires x402-specific values inside `metadata`:

```bash
curl -X POST http://localhost:8080/api/pay/x402/start \
  -H 'Content-Type: application/json' \
  -d '{
    "user_id": "user-1",
    "video_id": "video-1",
    "amount_cents": 10000,
    "currency": "USDC",
    "metadata": {
      "chain_id": "80002",
      "symbol": "USDC",
      "token_address": "0x0000000000000000000000000000000000000000",
      "payer_address": "0x1111111111111111111111111111111111111111"
    }
  }'
```

Required x402 metadata:

```text
chain_id
symbol
payer_address
```

Optional x402 metadata:

```text
token_address
```

The response returns the invoice under `invoice.raw`, including:

```text
invoice_uid
invoice_uid_bytes32
amount_wei
min_amount_wei
deadline
v
r
s
x402_contract
creator_wallet
```

x402 confirmation is still handled by the legacy endpoint until receipt verification is fully moved into the plugin:

```text
POST /api/pay/x402/confirm
```

## Confirm Payment

Default provider:

```bash
curl -X POST http://localhost:8080/api/pay/confirm \
  -H 'Content-Type: application/json' \
  -d '{
    "invoice_id": "invoice-1",
    "transaction_id": "tx-1",
    "provider_payload": {}
  }'
```

Explicit provider:

```bash
curl -X POST http://localhost:8080/api/pay/xendit/confirm \
  -H 'Content-Type: application/json' \
  -d '{
    "invoice_id": "invoice-1",
    "transaction_id": "tx-1",
    "provider_payload": {}
  }'
```

Provider skeletons currently return a clear not-yet-enabled error until each provider API integration is implemented.

## Folder Structure

```text
src/plugins/
├── mod.rs
└── payment/
    ├── mod.rs
    ├── models.rs
    ├── traits.rs
    ├── registry.rs
    └── providers/
        ├── mod.rs
        ├── x402.rs
        ├── paypal.rs
        ├── stripe.rs
        ├── midtrans.rs
        └── xendit.rs
```

## Core Concept

The application depends on a provider-neutral trait:

```rust
#[async_trait::async_trait]
pub trait PaymentPlugin: Send + Sync {
    fn provider_key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capability(&self) -> PaymentPluginCapability;
    async fn create_invoice(&self, request: CreateInvoiceRequest) -> anyhow::Result<Invoice>;
    async fn confirm_payment(&self, request: ConfirmPaymentRequest) -> anyhow::Result<PaymentResult>;
}
```

Each provider implements this trait:

```text
PaypalPaymentPlugin implements PaymentPlugin
StripePaymentPlugin implements PaymentPlugin
MidtransPaymentPlugin implements PaymentPlugin
XenditPaymentPlugin implements PaymentPlugin
X402PaymentPlugin implements PaymentPlugin
```

## Registry

`PaymentPluginRegistry` stores plugins as trait objects:

```rust
HashMap<String, Arc<dyn PaymentPlugin>>
```

This allows the application to select a payment provider at runtime:

```rust
let registry = PaymentPluginRegistry::from_env_with_pool(Some(pool.clone()));
let provider = registry.get("midtrans");
```

or use the configured default:

```rust
let provider = registry.default();
```

## Provider Responsibilities

Each payment plugin should handle:

```text
create invoice or checkout session
return payment redirect URL when applicable
validate provider notification or transaction confirmation
normalize provider status into PaymentStatus
return provider raw payload for auditing
```

## Provider Mapping

| Provider | Flow | Webhook Verification | Auto-Disburse |
|---|---|---|---|
| x402 | Blockchain authorization payload | Transaction receipt via RPC or event watcher | On-chain instant (smart contract splits at payment) |
| PayPal | Orders v2 API → redirect checkout → capture | `/v1/notifications/verify-webhook-signature` API | No — needs PayPal Payouts API |
| Stripe | Checkout Session → hosted page → redirect | HMAC-SHA256 on raw body vs `STRIPE_WEBHOOK_SECRET` | No — needs Stripe Connect |
| Midtrans | Snap API → hosted payment page | SHA-512(order_id + status_code + gross_amount + server_key) | No — no Midtrans payout API |
| Xendit | Invoice API → hosted invoice → callback | `x-callback-token` header check | Yes — Xendit Disbursements API (90% to creator bank) |

## Webhook Flow

When a provider sends a payment notification to `POST /api/pay/:provider/webhook`:

1. Raw request bytes and all headers are extracted.
2. The plugin's `confirm_payment()` verifies authenticity (signature/token).
3. On `PaymentStatus::Paid`: invoice is updated, `purchases` and `allowlist` rows are inserted.
4. For Xendit: `XenditPaymentPlugin::disburse_to_creator()` is called automatically.
5. The buyer now has permanent access to the video.

## Database Tables

The plugin system writes to:

| Table | Written by |
|---|---|
| `fiat_invoices` | `create_payment_invoice` — pre-inserted as `pending`; `affiliate_ref` stored separately |
| `fiat_invoices` | `handle_webhook` — updated to `paid` with `paid_at` and `provider_ref` |
| `purchases` | `handle_webhook` — one row per successful fiat payment |
| `allowlist` | `handle_webhook` — grants permanent playback access |
| `fiat_invoices` | `admin_disburse` — sets `disbursed_at` and `disburse_ref` |
| `wallet_transactions` | `commission::process_affiliate_commission` — after webhook confirms payment |
| `affiliate_commissions` | `commission::process_affiliate_commission` — commission audit row |

### Affiliate Referral Tracking

`fiat_invoices` has an `affiliate_ref TEXT` column (added by `migrations/029_affiliate.sql`). The referral username is stored at invoice creation time using a runtime SQL query (separate from the `sqlx::query!()` insert, so the offline cache is not affected):

```rust
// In create_invoice_with_provider, after the pre-insert:
sqlx::query("UPDATE fiat_invoices SET affiliate_ref = $1 WHERE invoice_uid = $2")
    .bind(ref_username).bind(&invoice_uid)
    .execute(&state.pool).await;
```

The webhook handler reads `affiliate_ref` back and calls `commission::process_affiliate_commission()` after access is granted.

→ See [AFFILIATE.md](AFFILIATE.md) for the full commission flow.

## Implementation Status

All phases complete:

| Phase | Description | Status |
|---|---|---|
| 1 | Plugin trait, model, registry, provider skeletons | Done |
| 2 | Generic HTTP handlers wired into Axum router | Done |
| 3 | Provider environment configuration and capability reporting | Done |
| 4 | Default provider routes | Done |
| 5 | x402 invoice creation in plugin | Done |
| 6 | x402 receipt verification in plugin | Done |
| 7 | Midtrans and Xendit full implementation | Done |
| 8 | PayPal and Stripe full implementation | Done |
| 9 | Webhook handler, DB writes, allowlist grant, Xendit auto-disburse | Done |

## Important Note

The plugin foundation is intentionally static, not dynamic native loading. This is safer for Rust production services because native dynamic plugin ABI compatibility is complex.

Recommended approach:

```text
trait + struct implementation + registry + environment configuration
```

Avoid this at the beginning:

```text
runtime .so/.dll loading
```

---

## Related Documentation

- [README.md](README.md) — platform overview
- [PAYMENT.md](PAYMENT.md) — all payment methods including wallet and X402
- [AFFILIATE.md](AFFILIATE.md) — how affiliate_ref flows through the plugin webhook
- [TECHNICAL_DOCUMENTATION.md](TECHNICAL_DOCUMENTATION.md) — code-level reference for handlers/payment_plugins.rs
