use anyhow::{bail, Result};

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug, Default)]
pub struct X402PaymentPlugin;

#[async_trait::async_trait]
impl PaymentPlugin for X402PaymentPlugin {
    fn provider_key(&self) -> &'static str { "x402" }
    fn display_name(&self) -> &'static str { "x402" }
    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().to_string(),
            display_name: self.display_name().to_string(),
            supports_redirect_checkout: false,
            supports_webhook_confirmation: false,
            supports_manual_confirmation: true,
            supported_currencies: vec!["USDC".into(), "MATIC".into(), "ETH".into()],
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        bail!("x402 plugin wrapper is registered; current x402 flow still lives in handlers/pay.rs")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        bail!("x402 plugin wrapper is registered; current x402 confirmation still lives in handlers/pay.rs")
    }
}
