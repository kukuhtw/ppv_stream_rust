// src/plugins/payment/providers/paypal.rs
//
// PayPal Orders API v2 — redirect-based payment.
//
// Flow:
//   1. POST /api/pay/paypal/start  → plugin.create_invoice() → returns approval URL
//   2. Buyer redirected to PayPal, approves payment
//   3. PayPal calls POST /api/pay/paypal/webhook (CHECKOUT.ORDER.APPROVED or PAYMENT.CAPTURE.COMPLETED)
//   4. confirm_payment() verifies via PayPal's verify-webhook-signature REST API
//
// Env vars required:
//   PAYPAL_CLIENT_ID       App Client ID (sandbox or live)
//   PAYPAL_CLIENT_SECRET   App Client Secret
//   PAYPAL_WEBHOOK_ID      Webhook ID from PayPal Developer Dashboard (needed for signature verify)
//   PAYPAL_ENV             "sandbox" (default) | "live"
//
// Auto-disburse: NOT implemented.
// All funds land in the platform's PayPal account.
// Creator payouts can be added later using PayPal Payouts API if creators supply their PayPal email.

use anyhow::{anyhow, bail, Result};
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
pub struct PaypalPaymentPlugin {
    config: PaymentProviderConfig,
    client_id: String,
    client_secret: String,
    webhook_id: String,
    api_base: String,
}

impl PaypalPaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env_or("PAYPAL_ENV", "sandbox");
        let api_base = match environment.as_str() {
            "live" | "production" => "https://api-m.paypal.com".to_string(),
            _ => "https://api-m.sandbox.paypal.com".to_string(),
        };
        let client_id = std::env::var("PAYPAL_CLIENT_ID").unwrap_or_default();
        let client_secret = std::env::var("PAYPAL_CLIENT_SECRET").unwrap_or_default();
        let webhook_id = std::env::var("PAYPAL_WEBHOOK_ID").unwrap_or_default();
        let required = [
            "PAYPAL_CLIENT_ID",
            "PAYPAL_CLIENT_SECRET",
            "PAYPAL_WEBHOOK_ID",
        ];
        Self {
            client_id,
            client_secret,
            webhook_id,
            api_base: api_base.clone(),
            config: PaymentProviderConfig::new(
                "paypal",
                environment,
                Some(api_base),
                required_env(&required),
                missing_env(&required),
            ),
        }
    }

    /// Get a short-lived OAuth2 Bearer token from PayPal.
    async fn access_token(&self) -> Result<String> {
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/oauth2/token", self.api_base))
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await
            .map_err(|e| anyhow!("paypal: token request failed: {e}"))?;

        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("paypal: token parse failed: {e}"))?;

        body["access_token"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow!("paypal: no access_token in response"))
    }

    /// Format amount as a decimal string for PayPal.
    /// PayPal uses 0-decimal for IDR/JPY, 2-decimal for USD/EUR.
    fn format_amount(amount_cents: i64, currency: &str) -> String {
        match currency.to_uppercase().as_str() {
            "IDR" | "JPY" | "HUF" | "TWD" => amount_cents.to_string(),
            _ => format!("{:.2}", amount_cents as f64 / 100.0),
        }
    }
}

impl Default for PaypalPaymentPlugin {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait::async_trait]
impl PaymentPlugin for PaypalPaymentPlugin {
    fn provider_key(&self) -> &'static str {
        "paypal"
    }
    fn display_name(&self) -> &'static str {
        "PayPal"
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
                "PayPal plugin not configured: {:?}",
                self.config.missing_env
            );
        }

        let token = self.access_token().await?;
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
        let amount_str = Self::format_amount(request.amount_cents, &request.currency);
        let currency = request.currency.to_uppercase();

        let order_body = json!({
            "intent": "CAPTURE",
            "purchase_units": [{
                "amount": { "currency_code": currency, "value": amount_str },
                // custom_id is returned in webhooks so we can map back to our invoice
                "custom_id": invoice_uid,
                "description": format!("Video: {}", video_title)
            }],
            "application_context": {
                "return_url": success_url,
                "cancel_url": cancel_url,
                "user_action": "PAY_NOW",
                "shipping_preference": "NO_SHIPPING"
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v2/checkout/orders", self.api_base))
            .bearer_auth(&token)
            .json(&order_body)
            .send()
            .await
            .map_err(|e| anyhow!("paypal: create order request failed: {e}"))?;

        let http_status = resp.status();
        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("paypal: order response parse error: {e}"))?;

        if !http_status.is_success() {
            bail!("paypal: API {http_status}: {body}");
        }

        let order_id = body["id"].as_str().unwrap_or("").to_string();
        let payment_url = body["links"]
            .as_array()
            .and_then(|links| links.iter().find(|l| l["rel"].as_str() == Some("approve")))
            .and_then(|l| l["href"].as_str())
            .map(String::from);

        Ok(Invoice {
            provider: self.provider_key().into(),
            invoice_id: invoice_uid,
            payment_url,
            amount_cents: request.amount_cents,
            currency: request.currency,
            status: PaymentStatus::Pending,
            raw: json!({ "order_id": order_id }),
        })
    }

    /// Verifies the webhook via PayPal's `verify-webhook-signature` REST API.
    ///
    /// Required headers (lowercase):
    ///   paypal-transmission-id, paypal-transmission-time, paypal-cert-url,
    ///   paypal-auth-algo, paypal-transmission-sig
    async fn confirm_payment(&self, request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured {
            bail!(
                "PayPal plugin not configured: {:?}",
                self.config.missing_env
            );
        }

        let payload = request
            .webhook_payload
            .ok_or_else(|| anyhow!("paypal: no webhook payload"))?;
        let h = &request.signature_headers;

        // Verify signature via PayPal's dedicated endpoint
        let token = self.access_token().await?;
        let verify_body = json!({
            "auth_algo":         h.get("paypal-auth-algo").cloned().unwrap_or_default(),
            "cert_url":          h.get("paypal-cert-url").cloned().unwrap_or_default(),
            "transmission_id":   h.get("paypal-transmission-id").cloned().unwrap_or_default(),
            "transmission_sig":  h.get("paypal-transmission-sig").cloned().unwrap_or_default(),
            "transmission_time": h.get("paypal-transmission-time").cloned().unwrap_or_default(),
            "webhook_id":        self.webhook_id.clone(),
            "webhook_event":     payload.clone()
        });

        let client = reqwest::Client::new();
        let vr: Value = client
            .post(format!(
                "{}/v1/notifications/verify-webhook-signature",
                self.api_base
            ))
            .bearer_auth(&token)
            .json(&verify_body)
            .send()
            .await
            .map_err(|e| anyhow!("paypal: webhook verify request failed: {e}"))?
            .json()
            .await
            .map_err(|e| anyhow!("paypal: webhook verify parse error: {e}"))?;

        if vr["verification_status"].as_str() != Some("SUCCESS") {
            bail!(
                "paypal: webhook signature invalid: {:?}",
                vr["verification_status"]
            );
        }

        let event_type = payload["event_type"].as_str().unwrap_or("");
        let resource = &payload["resource"];

        let status = match event_type {
            "CHECKOUT.ORDER.APPROVED" | "PAYMENT.CAPTURE.COMPLETED" => PaymentStatus::Paid,
            "PAYMENT.CAPTURE.DENIED" | "PAYMENT.CAPTURE.DECLINED" => PaymentStatus::Failed,
            "CHECKOUT.ORDER.CANCELLED" => PaymentStatus::Cancelled,
            _ => PaymentStatus::Unknown,
        };

        // custom_id = our invoice_uid (set when creating the order)
        let invoice_uid = resource["purchase_units"]
            .as_array()
            .and_then(|us| us.first())
            .and_then(|u| u["custom_id"].as_str())
            .unwrap_or_else(|| resource["custom_id"].as_str().unwrap_or(""))
            .to_string();

        let transaction_id = resource["id"].as_str().map(String::from);
        let paid_amount = resource["amount"]["value"]
            .as_str()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|f| (f * 100.0) as i64)
            .unwrap_or(0);
        let currency = resource["amount"]["currency_code"]
            .as_str()
            .unwrap_or("USD")
            .to_uppercase();

        Ok(PaymentResult {
            provider: self.provider_key().into(),
            invoice_id: invoice_uid,
            transaction_id,
            status,
            paid_amount_cents: paid_amount,
            currency,
            raw: payload,
        })
    }
}
