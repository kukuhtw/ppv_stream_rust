use anyhow::{bail, Result};

use crate::plugins::payment::{
    env::{env_or, missing_env, required_env},
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentProviderConfig, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct StripePaymentPlugin {
    config: PaymentProviderConfig,
}

impl StripePaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env_or("STRIPE_ENV", "test");
        let required = ["STRIPE_SECRET_KEY", "STRIPE_WEBHOOK_SECRET"];
        Self {
            config: PaymentProviderConfig::new(
                "stripe",
                environment,
                Some("https://api.stripe.com".to_string()),
                required_env(&required),
                missing_env(&required),
            ),
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
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.config.configured { bail!("Stripe plugin configuration is incomplete: {:?}", self.config.missing_env) }
        bail!("Stripe checkout API implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured { bail!("Stripe plugin configuration is incomplete: {:?}", self.config.missing_env) }
        bail!("Stripe confirmation implementation is not enabled yet")
    }
}
