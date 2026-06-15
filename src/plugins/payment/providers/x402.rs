use anyhow::{bail, Result};
use std::env;

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability, PaymentResult},
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct X402PaymentPlugin {
    configured: bool,
    missing_env: Vec<String>,
}

impl X402PaymentPlugin {
    pub fn from_env() -> Self {
        let required = ["X402_CONTRACT_ADDRESS", "X402_RPC_HTTP", "X402_ADMIN_PRIVKEY"];
        let missing_env = required
            .iter()
            .filter(|key| env::var(key).unwrap_or_default().is_empty())
            .map(|key| key.to_string())
            .collect::<Vec<_>>();
        Self {
            configured: missing_env.is_empty(),
            missing_env,
        }
    }
}

impl Default for X402PaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for X402PaymentPlugin {
    fn provider_key(&self) -> &'static str { "x402" }
    fn display_name(&self) -> &'static str { "x402" }
    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().to_string(),
            display_name: self.display_name().to_string(),
            configured: self.configured,
            environment: env::var("X402_CHAIN_ID").unwrap_or_else(|_| "evm".to_string()),
            api_base_url: env::var("X402_RPC_HTTP").ok(),
            supports_redirect_checkout: false,
            supports_webhook_confirmation: false,
            supports_manual_confirmation: true,
            supported_currencies: vec!["USDC".into(), "MATIC".into(), "ETH".into()],
            required_env: vec!["X402_CONTRACT_ADDRESS".into(), "X402_RPC_HTTP".into(), "X402_ADMIN_PRIVKEY".into()],
            missing_env: self.missing_env.clone(),
        }
    }
    async fn create_invoice(&self, _request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.configured { bail!("x402 plugin is not configured. Missing env: {:?}", self.missing_env) }
        bail!("x402 plugin wrapper is registered; current x402 flow still lives in handlers/pay.rs")
    }
    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.configured { bail!("x402 plugin is not configured. Missing env: {:?}", self.missing_env) }
        bail!("x402 plugin wrapper is registered; current x402 confirmation still lives in handlers/pay.rs")
    }
}
