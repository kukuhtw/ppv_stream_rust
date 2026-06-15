use anyhow::{bail, Result};

use crate::plugins::payment::{
    env::{env_or, missing_env, required_env},
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentProviderConfig, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct PaypalPaymentPlugin {
    config: PaymentProviderConfig,
}

impl PaypalPaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env_or("PAYPAL_ENV", "sandbox");
        let api_base_url = match environment.as_str() {
            "live" | "production" => "https://api-m.paypal.com".to_string(),
            _ => "https://api-m.sandbox.paypal.com".to_string(),
        };
        let required = ["PAYPAL_CLIENT_ID", "PAYPAL_CLIENT_SECRET"];
        Self {
            config: PaymentProviderConfig::new(
                "paypal",
                environment,
                Some(api_base_url),
                required_env(&required),
                missing_env(&required),
            ),
        }
    }
}

impl Default for PaypalPaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for PaypalPaymentPlugin {
    fn provider_key(&self) -> &'static str { "paypal" }
    fn display_name(&self) -> &'static str { "PayPal" }
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
        if !self.config.configured { bail!("PayPal plugin configuration is incomplete: {:?}", self.config.missing_env) }
        bail!("PayPal checkout API implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured { bail!("PayPal plugin configuration is incomplete: {:?}", self.config.missing_env) }
        bail!("PayPal confirmation implementation is not enabled yet")
    }
}
