use anyhow::{bail, Result};
use std::env;

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct PaypalPaymentPlugin {
    environment: String,
    api_base_url: String,
    configured: bool,
    missing_env: Vec<String>,
}

impl PaypalPaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env::var("PAYPAL_ENV").unwrap_or_else(|_| "sandbox".to_string());
        let api_base_url = match environment.as_str() {
            "live" | "production" => "https://api-m.paypal.com".to_string(),
            _ => "https://api-m.sandbox.paypal.com".to_string(),
        };
        let required = ["PAYPAL_CLIENT_ID", "PAYPAL_CLIENT_SECRET"];
        let missing_env = required
            .iter()
            .filter(|key| env::var(key).unwrap_or_default().is_empty())
            .map(|key| key.to_string())
            .collect::<Vec<_>>();
        Self {
            environment,
            api_base_url,
            configured: missing_env.is_empty(),
            missing_env,
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
    fn provider_key(&self) -> &'static str { "paypal" }
    fn display_name(&self) -> &'static str { "PayPal" }
    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().to_string(),
            display_name: self.display_name().to_string(),
            configured: self.configured,
            environment: self.environment.clone(),
            api_base_url: Some(self.api_base_url.clone()),
            supports_redirect_checkout: true,
            supports_webhook_confirmation: true,
            supports_manual_confirmation: false,
            supported_currencies: vec!["USD".into(), "EUR".into(), "IDR".into()],
            required_env: vec!["PAYPAL_CLIENT_ID".into(), "PAYPAL_CLIENT_SECRET".into()],
            missing_env: self.missing_env.clone(),
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.configured {
            bail!("PayPal plugin is not configured. Missing env: {:?}", self.missing_env)
        }
        bail!("PayPal checkout API implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.configured {
            bail!("PayPal plugin is not configured. Missing env: {:?}", self.missing_env)
        }
        bail!("PayPal confirmation implementation is not enabled yet")
    }
}
