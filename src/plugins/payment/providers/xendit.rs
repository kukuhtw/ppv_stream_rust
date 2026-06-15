use anyhow::{bail, Result};

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug, Default)]
pub struct XenditPaymentPlugin;

#[async_trait::async_trait]
impl PaymentPlugin for XenditPaymentPlugin {
    fn provider_key(&self) -> &'static str { "xendit" }
    fn display_name(&self) -> &'static str { "Xendit" }
    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().to_string(),
            display_name: self.display_name().to_string(),
            supports_redirect_checkout: true,
            supports_webhook_confirmation: true,
            supports_manual_confirmation: false,
            supported_currencies: vec!["IDR".into()],
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        bail!("Xendit plugin provider implementation is not enabled yet")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        bail!("Xendit plugin confirmation implementation is not enabled yet")
    }
}
