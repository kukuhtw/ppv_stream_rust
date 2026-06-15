# Payment Plugin Customization Architecture

This repository now has a payment plugin foundation under:

```text
src/plugins/payment/
```

The goal is to make payment providers configurable without hardcoding all provider logic directly inside `src/handlers/pay.rs`.

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

| Provider | Typical Flow | Confirmation |
|---|---|---|
| x402 | Blockchain authorization payload | Transaction receipt or watcher |
| PayPal | Redirect checkout | Order capture or notification |
| Stripe | Checkout Session or Payment Intent | Event notification |
| Midtrans | Hosted payment page or payment token | Notification callback |
| Xendit | Invoice or payment request | Callback notification |

## Migration Plan

### Phase 1

Create the plugin trait, model, registry, and provider skeleton.

Status: done.

### Phase 2

Expose generic HTTP handlers and wire them into the Axum router.

Status: done.

### Phase 3

Add provider environment configuration and capability reporting.

Status: done.

### Phase 4

Add default provider routes.

Status: done.

### Phase 5

Move x402 invoice creation into the x402 plugin.

Status: done.

### Phase 6

Move x402 receipt verification into the x402 plugin.

Status: next.

### Phase 7

Implement Midtrans and Xendit first for Indonesia payment support.

### Phase 8

Implement PayPal and Stripe for international users.

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
