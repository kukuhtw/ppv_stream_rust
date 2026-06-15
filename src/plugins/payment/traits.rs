// src/plugins/payment/traits.rs
//
// Provider-neutral payment plugin contract.
//
// Every payment integration must implement this trait. The rest of the
// application can call the trait without depending on PayPal, Stripe, Midtrans,
// Xendit, or x402 implementation details.

use anyhow::Result;

use super::models::{
    ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult,
};

#[async_trait::async_trait]
pub trait PaymentPlugin: Send + Sync {
    fn provider_key(&self) -> &'static str;

    fn display_name(&self) -> &'static str;

    fn capability(&self) -> PaymentPluginCapability;

    async fn create_invoice(&self, request: CreateInvoiceRequest) -> Result<Invoice>;

    async fn confirm_payment(&self, request: ConfirmPaymentRequest) -> Result<PaymentResult>;
}
