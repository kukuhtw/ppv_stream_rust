// src/plugins/payment/providers/midtrans.rs
//
// Midtrans Snap API — redirect-based payment for IDR (Indonesian Rupiah).
//
// Flow:
//   1. POST /api/pay/midtrans/start  → plugin.create_invoice() → returns snap redirect URL
//   2. Buyer redirected to Midtrans Snap payment page, pays
//   3. Midtrans calls POST /api/pay/midtrans/webhook (HTTP notification)
//   4. confirm_payment() verifies SHA512 signature, parses transaction_status
//
// Env vars required:
//   MIDTRANS_SERVER_KEY   Server key from Midtrans dashboard (sb- for sandbox, live for prod)
//   MIDTRANS_CLIENT_KEY   Client key (not used server-side but checked for config completeness)
//   MIDTRANS_ENV          "sandbox" (default) | "production"
//
// Webhook signature format:
//   SHA512( order_id + status_code + gross_amount + server_key ) == signature_key
//
// Auto-disburse: NOT possible — Midtrans has no native disbursement/payout API.
// All funds stay in the platform's Midtrans account.

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sha2::{Digest, Sha512};
use serde_json::{json, Value};

use crate::plugins::payment::{
    env::{env_or, missing_env, required_env},
    models::{
        ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability,
        PaymentProviderConfig, PaymentResult, PaymentStatus,
    },
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct MidtransPaymentPlugin {
    config:     PaymentProviderConfig,
    server_key: String,
    snap_base:  String,
}

impl MidtransPaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env_or("MIDTRANS_ENV", "sandbox");
        let (snap_base, api_base) = match environment.as_str() {
            "production" | "live" => (
                "https://app.midtrans.com/snap/v1".to_string(),
                "https://api.midtrans.com".to_string(),
            ),
            _ => (
                "https://app.sandbox.midtrans.com/snap/v1".to_string(),
                "https://api.sandbox.midtrans.com".to_string(),
            ),
        };
        let server_key = std::env::var("MIDTRANS_SERVER_KEY").unwrap_or_default();
        let required   = ["MIDTRANS_SERVER_KEY", "MIDTRANS_CLIENT_KEY"];
        Self {
            server_key,
            snap_base: snap_base.clone(),
            config: PaymentProviderConfig::new(
                "midtrans",
                environment,
                Some(api_base),
                required_env(&required),
                missing_env(&required),
            ),
        }
    }

    fn basic_auth_header(&self) -> String {
        let encoded = B64.encode(format!("{}:", self.server_key));
        format!("Basic {encoded}")
    }

    fn verify_signature(&self, order_id: &str, status_code: &str, gross_amount: &str) -> String {
        let mut hasher = Sha512::new();
        hasher.update(order_id.as_bytes());
        hasher.update(status_code.as_bytes());
        hasher.update(gross_amount.as_bytes());
        hasher.update(self.server_key.as_bytes());
        hex::encode(hasher.finalize())
    }
}

impl Default for MidtransPaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for MidtransPaymentPlugin {
    fn provider_key(&self)  -> &'static str { "midtrans" }
    fn display_name(&self)  -> &'static str { "Midtrans" }

    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider:                      self.provider_key().into(),
            display_name:                  self.display_name().into(),
            configured:                    self.config.configured,
            environment:                   self.config.environment.clone(),
            api_base_url:                  self.config.api_base_url.clone(),
            supports_redirect_checkout:    true,
            supports_webhook_confirmation: true,
            supports_manual_confirmation:  false,
            supported_currencies:          vec!["IDR".into()],
            required_env:                  self.config.required_env.clone(),
            missing_env:                   self.config.missing_env.clone(),
        }
    }

    async fn create_invoice(&self, request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.config.configured {
            bail!("Midtrans plugin not configured: {:?}", self.config.missing_env);
        }

        let invoice_uid  = request.metadata.get("invoice_uid").cloned().unwrap_or_default();
        let video_title  = request.metadata.get("video_title").cloned()
            .unwrap_or_else(|| "Video".into());
        let buyer_name   = request.metadata.get("buyer_name").cloned()
            .unwrap_or_else(|| "Buyer".into());
        let gross_amount = request.amount_cents; // IDR in full units, not cents
        let success_url  = request.success_url.as_deref()
            .unwrap_or("https://example.com/pay/success");
        let cancel_url   = request.cancel_url.as_deref()
            .unwrap_or("https://example.com/pay/cancel");

        let snap_body = json!({
            "transaction_details": {
                "order_id":     invoice_uid,
                "gross_amount": gross_amount
            },
            "item_details": [{
                "id":       request.video_id,
                "price":    gross_amount,
                "quantity": 1,
                "name":     video_title
            }],
            "customer_details": {
                "first_name": buyer_name,
                "email":      request.buyer_email.as_deref().unwrap_or("")
            },
            "callbacks": {
                "finish":  success_url,
                "error":   cancel_url,
                "pending": cancel_url
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/transactions", self.snap_base))
            .header("Authorization", self.basic_auth_header())
            .header("Content-Type", "application/json")
            .json(&snap_body)
            .send()
            .await
            .map_err(|e| anyhow!("midtrans: snap request failed: {e}"))?;

        let http_status = resp.status();
        let body: Value  = resp.json().await
            .map_err(|e| anyhow!("midtrans: snap response parse error: {e}"))?;

        if !http_status.is_success() {
            let msg = body["error_messages"].as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            bail!("midtrans: API {http_status}: {msg}");
        }

        let snap_token   = body["token"].as_str().unwrap_or("").to_string();
        let redirect_url = body["redirect_url"].as_str().map(String::from);

        Ok(Invoice {
            provider:     self.provider_key().into(),
            invoice_id:   invoice_uid,
            payment_url:  redirect_url,
            amount_cents: request.amount_cents,
            currency:     "IDR".into(),
            status:       PaymentStatus::Pending,
            raw:          json!({ "snap_token": snap_token }),
        })
    }

    async fn confirm_payment(&self, request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured {
            bail!("Midtrans plugin not configured: {:?}", self.config.missing_env);
        }

        let payload = request.webhook_payload
            .ok_or_else(|| anyhow!("midtrans: no webhook payload"))?;

        let order_id     = payload["order_id"].as_str().unwrap_or("");
        let status_code  = payload["status_code"].as_str().unwrap_or("");
        let gross_amount = payload["gross_amount"].as_str().unwrap_or("0");
        let sig_from_mt  = payload["signature_key"].as_str().unwrap_or("");

        let expected = self.verify_signature(order_id, status_code, gross_amount);
        if expected != sig_from_mt {
            bail!("midtrans: webhook signature mismatch");
        }

        let txn_status = payload["transaction_status"].as_str().unwrap_or("");
        let fraud      = payload["fraud_status"].as_str().unwrap_or("accept");

        let status = match (txn_status, fraud) {
            ("capture", "accept") | ("capture", "challenge") | ("settlement", _) => {
                PaymentStatus::Paid
            }
            ("deny", _) | ("failure", _) => PaymentStatus::Failed,
            ("cancel", _)                => PaymentStatus::Cancelled,
            ("expire", _)                => PaymentStatus::Expired,
            ("pending", _)               => PaymentStatus::Pending,
            _                            => PaymentStatus::Unknown,
        };

        let transaction_id = payload["transaction_id"].as_str().map(String::from);
        // gross_amount from Midtrans is IDR as "28500.00" — trim the ".00"
        let paid_amount = gross_amount
            .split('.')
            .next()
            .unwrap_or("0")
            .parse::<i64>()
            .unwrap_or(0);

        Ok(PaymentResult {
            provider:          self.provider_key().into(),
            invoice_id:        order_id.into(),
            transaction_id,
            status,
            paid_amount_cents: paid_amount,
            currency:          "IDR".into(),
            raw:               payload,
        })
    }
}
