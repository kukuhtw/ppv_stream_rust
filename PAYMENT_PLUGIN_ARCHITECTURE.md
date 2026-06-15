# Payment Plugin Customization Architecture

This repository now has a payment plugin foundation under:

```text
src/plugins/payment/
```

The goal is to make payment providers configurable without hardcoding all provider logic directly inside `src/handlers/pay.rs`.

## Supported Provider Targets

The initial plugin skeleton supports these provider names:

```text
x402
paypal
stripe
midtrans
xendit
```

The current implementation creates the extension points first. Provider API calls can then be migrated one by one from handler logic into plugin implementations.

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

```dotenv
PAYMENT_PLUGINS=x402
PAYMENT_DEFAULT_PROVIDER=x402
```

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

The application should depend on a provider-neutral trait:

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
let registry = PaymentPluginRegistry::from_env();
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
validate webhook or transaction confirmation
normalize provider status into PaymentStatus
return provider raw payload for auditing
```

## Provider Mapping

| Provider | Typical Flow | Confirmation |
|---|---|---|
| x402 | Blockchain authorization payload | Transaction receipt or watcher |
| PayPal | Redirect checkout | Webhook or order capture |
| Stripe | Checkout Session or Payment Intent | Webhook |
| Midtrans | Hosted payment page or payment token | Notification callback |
| Xendit | Invoice or payment request | Callback webhook |

## Migration Plan

### Phase 1

Create the plugin trait, model, registry, and provider skeleton.

Status: done.

### Phase 2

Move current x402 logic from:

```text
src/handlers/pay.rs
```

into:

```text
src/plugins/payment/providers/x402.rs
```

### Phase 3

Update payment handlers to call the registry instead of provider-specific functions.

Suggested generic endpoints:

```text
GET  /api/pay/providers
POST /api/pay/:provider/start
POST /api/pay/:provider/confirm
POST /api/pay/:provider/webhook
```

### Phase 4

Implement Midtrans and Xendit first for Indonesia payment support.

### Phase 5

Implement PayPal and Stripe for international users.

## Recommended Handler Design

Instead of provider-specific routes only:

```text
/api/pay/x402/start
/api/pay/x402/confirm
```

add generic routes:

```text
/api/pay/{provider}/start
/api/pay/{provider}/confirm
```

Then route internally:

```rust
let plugin = registry
    .get(provider)
    .ok_or_else(|| anyhow!("payment plugin not found"))?;

let invoice = plugin.create_invoice(request).await?;
```

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

## Next Implementation Target

The next best step is to create generic HTTP handlers that use `PaymentPluginRegistry`, then gradually move the existing x402 code into the x402 plugin.
