use anyhow::{bail, Result};

use crate::plugins::payment::{
    env::{env_or, missing_env, required_env},
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentProviderConfig, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct XenditPaymentPlugin {
    config: PaymentProviderConfig,
}

impl XenditPaymentPlugin {
    pub fn from_env() -> Self {
        let environment = env_or("XENDIT_ENV", "test");
        let required = ["XENDIT_SECRET_KEY", "XENDIT_WEBHOOK_TOKEN"];
        Self {
            config: PaymentProviderConfig::new(
                "xendit",
                environment,
                Some("https://api.xendit.co".to_string()),
                required_env(&required),
                missing_env(&required),
            ),
        }
    }
}

impl Default for XenditPaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for XenditPaymentPlugin {
    fn provider_key(&self) -> &'static str { "xendit" }
    fn display_name(&self) -> &'static str { "Xendit" }
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
            supported_currencies: vec!["IDR".into()],
            required_env: self.config.required_env.clone(),
            missing_env: self.config.missing_env.clone(),
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.config.configured { bail!("Xendit plugin configuration is incomplete: {:?}", self.config.missing_env) }
        bail!("Xendit invoice API implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.config.configured { bail!("Xendit plugin configuration is incomplete: {:?}", self.config.missing_env) }
        bail!("Xendit confirmation implementation is not enabled yet")
    }
}
