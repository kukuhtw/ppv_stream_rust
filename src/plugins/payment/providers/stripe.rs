use anyhow::{bail, Result};
use std::env;

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct StripePaymentPlugin {
    environment: String,
    configured: bool,
    missing_env: Vec<String>,
}

impl StripePaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env::var("STRIPE_ENV").unwrap_or_else(|_| "test".to_string());
        let required = ["STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET"];
        let missing_env = required
            .iter()
            .filter(|key| env::var(key).unwrap_or_default().is_empty())
            .map(|key| key.to_string())
            .collect::<Vec<_>>();
        Self {
            environment,
            configured: missing_env.is_empty(),
            missing_env,
        }
    }
}

impl Default for StripePaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for StripePaymentPlugin {
    fn provider_key(&self) -> &'static str { "stripe" }
    fn display_name(&self) -> &'static str { "Stripe" }
    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().to_string(),
            display_name: self.display_name().to_string(),
            configured: self.configured,
            environment: self.environment.clone(),
            api_base_url: Some("https://api.stripe.com".to_string()),
            supports_redirect_checkout: true,
            supports_webhook_confirmation: true,
            supports_manual_confirmation: false,
            supported_currencies: vec!["USD".into(), "EUR".into(), "IDR".into()],
            required_env: vec!["STRIPE_SECRET_KEY".into(), "STRIPE_WEBHOOK_SECRET".into()],
            missing_env: self.missing_env.clone(),
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.configured { bail!("Stripe plugin is not configured. Missing env: {:?}", self.missing_env) }
        bail!("Stripe checkout API implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.configured { bail!("Stripe plugin is not configured. Missing env: {:?}", self.missing_env) }
        bail!("Stripe confirmation implementation is not enabled yet")
    }
}
