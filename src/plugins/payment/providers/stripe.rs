// src/plugins/payment/providers/stripe.rs
//
// Stripe Checkout Sessions — redirect-based payment.
//
// Flow:
//   1. POST /api/pay/stripe/start  → plugin.create_invoice() → returns session.url
//   2. Buyer redirected to Stripe, pays
//   3. Stripe calls POST /api/pay/stripe/webhook  (checkout.session.completed)
//   4. confirm_payment() verifies Stripe-Signature HMAC-SHA256
//
// Env vars required:
//   STRIPE_SECRET_KEY       sk_test_... / sk_live_...
//   STRIPE_WEBHOOK_SECRET   whsec_...  (from Stripe Dashboard → Webhooks)
//   STRIPE_ENV              "test" (default) | "live"
//
// Auto-disburse: NOT implemented.
// All funds land in the platform's Stripe account.
// Creator payouts need Stripe Connect (out of scope for v1).

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

use crate::plugins::payment::{
    env::{env_or, missing_env, required_env},
    models::{
        ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability,
        PaymentProviderConfig, PaymentResult, PaymentStatus,
    },
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct StripePaymentPlugin {
    config: PaymentProviderConfig,
    secret_key: String,
    webhook_secret: String,
}

impl StripePaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env_or("STRIPE_ENV", "test");
        let secret_key = std::env::var("STRIPE_SECRET_KEY").unwrap_or_default();
        let webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default();
        let required = ["STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET"];
        Self {
            secret_key,
            webhook_secret,
            config: PaymentProviderConfig::new(
                "stripe",
                environment,
                Some("https://api.stripe.com".into()),
                required_env(&required),
                missing_env(&required),
            ),
        }
    }

    /// Verify `Stripe-Signature` header using HMAC-SHA256.
    /// `raw_payload` must be the exact bytes received from Stripe (not re-serialised JSON).
    fn verify_signature(&self, raw_payload: &[u8], sig_header: &str) -> Result<()> {
        let mut ts: Option<&str> = None;
        let mut v1_sigs: Vec<&str> = Vec::new();
        for segment in sig_header.split(',') {
            if let Some(v) = segment.strip_prefix("t=") {
                ts = Some(v);
            }
            if let Some(v) = segment.strip_prefix("v1=") {
                v1_sigs.push(v);
            }
        }
        let ts = ts.ok_or_else(|| anyhow!("stripe: missing timestamp in Stripe-Signature"))?;
        let signed = [ts.as_bytes(), b".", raw_payload].concat();

        let mut mac = Hmac::<Sha256>::new_from_slice(self.webhook_secret.as_bytes())
            .map_err(|_| anyhow!("stripe: invalid webhook secret"))?;
        mac.update(&signed);
        let computed = hex::encode(mac.finalize().into_bytes());

        if v1_sigs.contains(&computed.as_str()) {
            Ok(())
        } else {
            bail!("stripe: Stripe-Signature verification failed")
        }
    }
}

impl Default for StripePaymentPlugin {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait::async_trait]
impl PaymentPlugin for StripePaymentPlugin {
    fn provider_key(&self) -> &'static str {
        "stripe"
    }
    fn display_name(&self) -> &'static str {
        "Stripe"
    }

    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().into(),
            display_name: self.display_name().into(),
            configured: self.config.configured,
            environment: self.config.environment.clone(),
            api_base_url: self.config.api_base_url.clone(),
            supports_redirect_checkout: true,
            supports_webhook_confirmation: true,
            supports_manual_confirmation: false,
            supported_currencies: vec!["USD".into(), "EUR".into(), "IDR".into()],
            required_env: self.config.required_env.clone(),
            missing_env: self.config.missing_env.clone(),
        }
    }

    async fn create_invoice(&self, request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.config.configured {
            bail!(
                "Stripe plugin not configured: {:?}",
                self.config.missing_env
            );
        }

        let invoice_uid = request
            .metadata
            .get("invoice_uid")
            .cloned()
            .unwrap_or_default();
        let video_title = request
            .metadata
            .get("video_title")
            .cloned()
            .unwrap_or_else(|| "Video".into());
        let success_url = request
            .success_url
            .as_deref()
            .unwrap_or("https://example.com/pay/success");
        let cancel_url = request
            .cancel_url
            .as_deref()
            .unwrap_or("https://example.com/pay/cancel");
        let currency = request.currency.to_lowercase();

        // Stripe amounts are always in the smallest unit (cents for USD/EUR, IDR for IDR)
        let mut params: Vec<(String, String)> = vec![
            ("mode".into(), "payment".into()),
            ("line_items[0][price_data][currency]".into(), currency),
            (
                "line_items[0][price_data][unit_amount]".into(),
                request.amount_cents.to_string(),
            ),
            (
                "line_items[0][price_data][product_data][name]".into(),
                video_title,
            ),
            ("line_items[0][quantity]".into(), "1".into()),
            (
                "success_url".into(),
                format!("{}?session_id={{CHECKOUT_SESSION_ID}}", success_url),
            ),
            ("cancel_url".into(), cancel_url.into()),
            ("metadata[invoice_uid]".into(), invoice_uid.clone()),
            ("metadata[video_id]".into(), request.video_id.clone()),
            ("metadata[user_id]".into(), request.user_id.clone()),
        ];
        if let Some(email) = &request.buyer_email {
            if !email.is_empty() {
                params.push(("customer_email".into(), email.clone()));
            }
        }

        let client = reqwest::Client::new();
        let resp = client
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Some(""))
            .form(&params)
            .send()
            .await
            .map_err(|e| anyhow!("stripe: HTTP error: {e}"))?;

        let http_status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("stripe: response parse error: {e}"))?;

        if !http_status.is_success() {
            let msg = body["error"]["message"].as_str().unwrap_or("unknown");
            bail!("stripe: API {http_status}: {msg}");
        }

        let session_id = body["id"].as_str().unwrap_or("").to_string();
        let payment_url = body["url"].as_str().map(String::from);

        Ok(Invoice {
            provider: self.provider_key().into(),
            invoice_id: invoice_uid,
            payment_url,
            amount_cents: request.amount_cents,
            currency: request.currency,
            status: PaymentStatus::Pending,
            raw: json!({ "session_id": session_id }),
        })
    }

    /// Called by the webhook handler.
    ///
    /// The webhook handler stores the original raw bytes as base64 under `__raw__` inside
    /// the JSON value so HMAC can be computed against the exact received bytes.
    async fn confirm_payment(&self, request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured {
            bail!(
                "Stripe plugin not configured: {:?}",
                self.config.missing_env
            );
        }

        let payload = request
            .webhook_payload
            .ok_or_else(|| anyhow!("stripe: no webhook payload"))?;

        // Recover original raw bytes stored by the webhook handler
        let raw: Vec<u8> = payload
            .get("__raw__")
            .and_then(|v| v.as_str())
            .map(|b64| B64.decode(b64).unwrap_or_default())
            .unwrap_or_else(|| serde_json::to_vec(&payload).unwrap_or_default());

        let sig = request
            .signature_headers
            .get("stripe-signature")
            .or_else(|| request.signature_headers.get("Stripe-Signature"))
            .ok_or_else(|| anyhow!("stripe: missing Stripe-Signature header"))?;

        self.verify_signature(&raw, sig)?;

        let event: Value = serde_json::from_slice(&raw)?;
        let event_type = event["type"].as_str().unwrap_or("");
        let obj = &event["data"]["object"];

        let status = match event_type {
            "checkout.session.completed" => {
                if obj["payment_status"].as_str() == Some("paid") {
                    PaymentStatus::Paid
                } else {
                    PaymentStatus::Pending
                }
            }
            "payment_intent.succeeded" => PaymentStatus::Paid,
            "payment_intent.payment_failed" => PaymentStatus::Failed,
            "checkout.session.expired" => PaymentStatus::Expired,
            _ => PaymentStatus::Unknown,
        };

        let invoice_uid = obj["metadata"]["invoice_uid"].as_str().unwrap_or("").into();
        let transaction_id = obj["payment_intent"].as_str().map(String::from);
        let paid_amount = obj["amount_total"].as_i64().unwrap_or(0);
        let currency = obj["currency"].as_str().unwrap_or("usd").to_uppercase();

        Ok(PaymentResult {
            provider: self.provider_key().into(),
            invoice_id: invoice_uid,
            transaction_id,
            status,
            paid_amount_cents: paid_amount,
            currency,
            raw: event,
        })
    }
}
