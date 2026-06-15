// src/plugins/payment/registry.rs
//
// Runtime registry for payment plugins.

use std::{collections::HashMap, env, sync::Arc};

use sqlx::PgPool;

use super::{
    providers::{
        midtrans::MidtransPaymentPlugin,
        paypal::PaypalPaymentPlugin,
        stripe::StripePaymentPlugin,
        x402::X402PaymentPlugin,
        xendit::XenditPaymentPlugin,
    },
    traits::PaymentPlugin,
};

#[derive(Clone, Default)]
pub struct PaymentPluginRegistry {
    plugins: HashMap<String, Arc<dyn PaymentPlugin>>,
    default_provider: Option<String>,
}

impl PaymentPluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_env() -> Self {
        Self::from_env_with_pool(None)
    }

    pub fn from_env_with_pool(pool: Option<PgPool>) -> Self {
        let enabled = env::var("PAYMENT_PLUGINS")
            .unwrap_or_else(|_| "x402,paypal,stripe,midtrans,xendit".to_string());
        let default_provider = env::var("PAYMENT_DEFAULT_PROVIDER").ok();

        let mut registry = Self::new();
        for provider in enabled.split(',').map(|value| value.trim().to_ascii_lowercase()) {
            match provider.as_str() {
                "paypal" => registry.register(Arc::new(PaypalPaymentPlugin::from_env())),
                "stripe" => registry.register(Arc::new(StripePaymentPlugin::from_env())),
                "midtrans" => registry.register(Arc::new(MidtransPaymentPlugin::from_env())),
                "xendit" => registry.register(Arc::new(XenditPaymentPlugin::from_env())),
                "x402" => registry.register(Arc::new(X402PaymentPlugin::from_env_with_pool(pool.clone()))),
                "" => {}
                _ => tracing::warn!("unknown payment plugin configured: {}", provider),
            }
        }

        registry.default_provider = default_provider.or_else(|| registry.names().first().cloned());
        registry
    }

    pub fn register(&mut self, plugin: Arc<dyn PaymentPlugin>) {
        self.plugins.insert(plugin.provider_key().to_string(), plugin);
    }

    pub fn get(&self, provider: &str) -> Option<Arc<dyn PaymentPlugin>> {
        self.plugins.get(&provider.to_ascii_lowercase()).cloned()
    }

    pub fn default(&self) -> Option<Arc<dyn PaymentPlugin>> {
        self.default_provider.as_deref().and_then(|provider| self.get(provider))
    }

    pub fn default_provider_name(&self) -> Option<String> {
        self.default_provider.clone()
    }

    pub fn names(&self) -> Vec<String> {
        let mut names = self.plugins.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
    }
}
