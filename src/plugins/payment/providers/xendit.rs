// src/plugins/payment/providers/xendit.rs
//
// Xendit Invoice API — redirect-based payment (primarily IDR, also PHP/USD).
//
// Flow:
//   1. POST /api/pay/xendit/start  → plugin.create_invoice() → returns invoice_url
//   2. Buyer redirected to Xendit-hosted payment page
//   3. Xendit calls POST /api/pay/xendit/webhook on payment completion
//   4. confirm_payment() checks x-callback-token header, returns result
//   5. Webhook handler calls disburse_to_creator() for 90% auto-disbursement
//
// Env vars required:
//   XENDIT_SECRET_KEY       Secret API key (Money-In operations)
//   XENDIT_WEBHOOK_TOKEN    Callback verification token (set in Xendit dashboard)
//   XENDIT_ENV              "test" (default) | "production"
//
// Auto-disburse: YES — Xendit Disbursement API supports sending funds to Indonesian bank accounts.
//
// Creator bank account format (stored in users.bank_account):
//   "BCA 1234567890 a/n Nama Lengkap"
//   Parsed as: bank_code="BCA", account_number="1234567890", account_holder_name="Nama Lengkap"

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
pub struct XenditPaymentPlugin {
    config:        PaymentProviderConfig,
    secret_key:    String,
    webhook_token: String,
    api_base:      String,
}

impl XenditPaymentPlugin {
    pub fn from_env() -> Self {
        let environment   = env_or("XENDIT_ENV", "test");
        let api_base      = "https://api.xendit.co".to_string();
        let secret_key    = std::env::var("XENDIT_SECRET_KEY").unwrap_or_default();
        let webhook_token = std::env::var("XENDIT_WEBHOOK_TOKEN").unwrap_or_default();
        let required      = ["XENDIT_SECRET_KEY", "XENDIT_WEBHOOK_TOKEN"];
        Self {
            secret_key,
            webhook_token,
            api_base: api_base.clone(),
            config: PaymentProviderConfig::new(
                "xendit",
                environment,
                Some(api_base),
                required_env(&required),
                missing_env(&required),
            ),
        }
    }

    /// Parse creator's bank_account string into (bank_code, account_number, holder_name).
    ///
    /// Expected format: "BCA 1234567890 a/n Nama Lengkap"
    fn parse_bank_account(bank_account: &str) -> Option<(String, String, String)> {
        let parts: Vec<&str> = bank_account.splitn(2, "a/n").collect();
        if parts.len() != 2 { return None; }
        let left: Vec<&str> = parts[0].split_whitespace().collect();
        if left.len() < 2 { return None; }
        let bank_code      = left[0].to_uppercase();
        let account_number = left[1].to_string();
        let holder_name    = parts[1].trim().to_string();
        if holder_name.is_empty() || account_number.is_empty() { return None; }
        Some((bank_code, account_number, holder_name))
    }

    /// Send 90% of the paid amount to the creator via Xendit Disbursement API.
    /// Called by the webhook handler after a successful payment is confirmed.
    pub async fn disburse_to_creator(
        &self,
        creator_bank_account: &str,
        amount_idr:           i64,
        invoice_uid:          &str,
    ) -> Result<Value> {
        let (bank_code, account_number, holder_name) =
            Self::parse_bank_account(creator_bank_account)
                .ok_or_else(|| anyhow!("xendit: invalid bank_account: {creator_bank_account}"))?;

        let creator_amount = (amount_idr as f64 * 0.9) as i64;

        let body = json!({
            "external_id":         format!("{invoice_uid}-creator"),
            "amount":              creator_amount,
            "bank_code":           bank_code,
            "account_holder_name": holder_name,
            "account_number":      account_number,
            "description":         format!("PPV creator payout for {invoice_uid}")
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/disbursements", self.api_base))
            .basic_auth(&self.secret_key, Some(""))
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("xendit: disbursement request failed: {e}"))?;

        let http_status = resp.status();
        let result: Value = resp.json().await
            .map_err(|e| anyhow!("xendit: disbursement parse error: {e}"))?;

        if !http_status.is_success() {
            let msg = result["message"].as_str().unwrap_or("unknown");
            bail!("xendit: disbursement API {http_status}: {msg}");
        }

        Ok(result)
    }
}

impl Default for XenditPaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for XenditPaymentPlugin {
    fn provider_key(&self)  -> &'static str { "xendit" }
    fn display_name(&self)  -> &'static str { "Xendit" }

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
            supported_currencies:          vec!["IDR".into(), "PHP".into(), "USD".into()],
            required_env:                  self.config.required_env.clone(),
            missing_env:                   self.config.missing_env.clone(),
        }
    }

    async fn create_invoice(&self, request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.config.configured {
            bail!("Xendit plugin not configured: {:?}", self.config.missing_env);
        }

        let invoice_uid = request.metadata.get("invoice_uid").cloned().unwrap_or_default();
        let video_title = request.metadata.get("video_title").cloned()
            .unwrap_or_else(|| "Video".into());
        let buyer_name  = request.metadata.get("buyer_name").cloned()
            .unwrap_or_else(|| "Buyer".into());
        let success_url = request.success_url.as_deref()
            .unwrap_or("https://example.com/pay/success");
        let cancel_url  = request.cancel_url.as_deref()
            .unwrap_or("https://example.com/pay/cancel");
        let currency    = request.currency.to_uppercase();

        let invoice_body = json!({
            "external_id":          invoice_uid,
            "amount":               request.amount_cents,
            "currency":             currency,
            "description":          format!("Video: {}", video_title),
            "payer_email":          request.buyer_email.as_deref().unwrap_or(""),
            "customer":             { "given_names": buyer_name },
            "success_redirect_url": success_url,
            "failure_redirect_url": cancel_url
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v2/invoices", self.api_base))
            .basic_auth(&self.secret_key, Some(""))
            .json(&invoice_body)
            .send()
            .await
            .map_err(|e| anyhow!("xendit: invoice request failed: {e}"))?;

        let http_status = resp.status();
        let body: Value  = resp.json().await
            .map_err(|e| anyhow!("xendit: invoice parse error: {e}"))?;

        if !http_status.is_success() {
            let msg = body["message"].as_str().unwrap_or("unknown");
            bail!("xendit: API {http_status}: {msg}");
        }

        let xendit_id   = body["id"].as_str().unwrap_or("").to_string();
        let payment_url = body["invoice_url"].as_str().map(String::from);

        Ok(Invoice {
            provider:     self.provider_key().into(),
            invoice_id:   invoice_uid,
            payment_url,
            amount_cents: request.amount_cents,
            currency:     request.currency,
            status:       PaymentStatus::Pending,
            raw:          json!({ "xendit_invoice_id": xendit_id }),
        })
    }

    /// Verifies x-callback-token header, then parses `status` from the Xendit Invoice webhook.
    ///
    /// After this returns PaymentStatus::Paid, the webhook handler should call
    /// `disburse_to_creator()` for the 90% creator auto-payout.
    async fn confirm_payment(&self, request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured {
            bail!("Xendit plugin not configured: {:?}", self.config.missing_env);
        }

        let payload = request.webhook_payload
            .ok_or_else(|| anyhow!("xendit: no webhook payload"))?;

        let token = request.signature_headers
            .get("x-callback-token")
            .ok_or_else(|| anyhow!("xendit: missing x-callback-token header"))?;

        if token != &self.webhook_token {
            bail!("xendit: x-callback-token mismatch");
        }

        let xendit_status = payload["status"].as_str().unwrap_or("PENDING");
        let status = match xendit_status {
            "PAID" | "SETTLED" => PaymentStatus::Paid,
            "EXPIRED"          => PaymentStatus::Expired,
            _                  => PaymentStatus::Pending,
        };

        let invoice_uid    = payload["external_id"].as_str().unwrap_or("").to_string();
        let transaction_id = payload["id"].as_str().map(String::from);
        let paid_amount    = payload["paid_amount"].as_i64()
            .or_else(|| payload["amount"].as_i64())
            .unwrap_or(0);
        let currency = payload["currency"].as_str().unwrap_or("IDR").to_uppercase();

        Ok(PaymentResult {
            provider:          self.provider_key().into(),
            invoice_id:        invoice_uid,
            transaction_id,
            status,
            paid_amount_cents: paid_amount,
            currency,
            raw:               payload,
        })
    }
}
