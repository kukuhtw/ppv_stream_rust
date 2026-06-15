use anyhow::{bail, Result};
use std::env;

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct MidtransPaymentPlugin {
    environment: String,
    api_base_url: String,
    configured: bool,
    missing_env: Vec<String>,
}

impl MidtransPaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env::var("MIDTRANS_ENV").unwrap_or_else(|_| "sandbox".to_string());
        let api_base_url = match environment.as_str() {
            "live" | "production" => "https://api.midtrans.com".to_string(),
            _ => "https://api.sandbox.midtrans.com".to_string(),
        };
        let required = ["MIDTRANS_SERVER_KEY", "MIDTRANS_CLIENT_KEY"];
        let missing_env = required
            .iter()
            .filter(|key| env::var(key).unwrap_or_default().is_empty())
            .map(|key| key.to_string())
            .collect::<Vec<_>>();
        Self { environment, api_base_url, configured: missing_env.is_empty(), missing_env }
    }
}

impl Default for MidtransPaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for MidtransPaymentPlugin {
    fn provider_key(&self) -> &'static str { "midtrans" }
    fn display_name(&self) -> &'static str { "Midtrans" }
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
            supported_currencies: vec!["IDR".into()],
            required_env: vec!["MIDTRANS_SERVER_KEY".into(), "MIDTRANS_CLIENT_KEY".into()],
            missing_env: self.missing_env.clone(),
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.configured { bail!("Midtrans plugin is not configured. Missing env: {:?}", self.missing_env) }
        bail!("Midtrans provider API implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.configured { bail!("Midtrans plugin is not configured. Missing env: {:?}", self.missing_env) }
        bail!("Midtrans confirmation implementation is not enabled yet")
    }
}
