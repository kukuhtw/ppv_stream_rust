// src/plugins/payment/registry.rs
//
// Runtime registry for payment plugins.

use std::{collections::HashMap, env, sync::Arc};

use sqlx::PgPool;

use super::{
    providers::{
        midtrans::MidtransPaymentPlugin, paypal::PaypalPaymentPlugin, stripe::StripePaymentPlugin,
        x402::X402PaymentPlugin, xendit::XenditPaymentPlugin,
    },
    traits::PaymentPlugin,
};
use crate::payment_settings::{load_payment_settings, PaymentSettings};

#[derive(Clone, Default)]
pub struct PaymentPluginRegistry {
    plugins: HashMap<String, Arc<dyn PaymentPlugin>>,
    default_provider: Option<String>,
}

impl PaymentPluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            default_provider: None,
        }
    }

    #[allow(dead_code)]
    pub fn from_env() -> Self {
        Self::from_env_with_pool(None)
    }

    pub fn from_env_with_pool(pool: Option<PgPool>) -> Self {
        let enabled = env::var("PAYMENT_PLUGINS")
            .unwrap_or_else(|_| "x402,paypal,stripe,midtrans,xendit".to_string());
        let default_provider = env::var("PAYMENT_DEFAULT_PROVIDER").ok();

        let mut registry = Self::new();
        for provider in enabled
            .split(',')
            .map(|value| value.trim().to_ascii_lowercase())
        {
            match provider.as_str() {
                "paypal" => registry.register(Arc::new(PaypalPaymentPlugin::from_env())),
                "stripe" => registry.register(Arc::new(StripePaymentPlugin::from_env())),
                "midtrans" => registry.register(Arc::new(MidtransPaymentPlugin::from_env())),
                "xendit" => registry.register(Arc::new(XenditPaymentPlugin::from_env())),
                "x402" => registry.register(Arc::new(X402PaymentPlugin::from_env_with_pool(
                    pool.clone(),
                ))),
                "" => {}
                _ => tracing::warn!("unknown payment plugin configured: {}", provider),
            }
        }

        registry.default_provider = default_provider.or_else(|| registry.names().first().cloned());
        registry
    }

    pub async fn from_runtime_with_pool(pool: PgPool) -> Self {
        let settings = load_payment_settings(&pool).await;
        Self::from_settings(Some(pool), settings)
    }

    pub fn capabilities_from_env_with_pool(
        pool: Option<PgPool>,
    ) -> Vec<super::models::PaymentPluginCapability> {
        let plugins: Vec<Arc<dyn PaymentPlugin>> = vec![
            Arc::new(PaypalPaymentPlugin::from_env()),
            Arc::new(StripePaymentPlugin::from_env()),
            Arc::new(MidtransPaymentPlugin::from_env()),
            Arc::new(XenditPaymentPlugin::from_env()),
            Arc::new(X402PaymentPlugin::from_env_with_pool(pool)),
        ];

        plugins
            .into_iter()
            .map(|plugin| plugin.capability())
            .collect()
    }

    pub fn from_all_env_known_with_pool(pool: Option<PgPool>) -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(PaypalPaymentPlugin::from_env()));
        registry.register(Arc::new(StripePaymentPlugin::from_env()));
        registry.register(Arc::new(MidtransPaymentPlugin::from_env()));
        registry.register(Arc::new(XenditPaymentPlugin::from_env()));
        registry.register(Arc::new(X402PaymentPlugin::from_env_with_pool(pool)));
        registry.default_provider = registry.names().first().cloned();
        registry
    }

    fn from_settings(pool: Option<PgPool>, settings: PaymentSettings) -> Self {
        let mut registry = Self::new();

        if settings.paypal_enabled {
            registry.register(Arc::new(PaypalPaymentPlugin::from_env()));
        }
        if settings.stripe_enabled {
            registry.register(Arc::new(StripePaymentPlugin::from_env()));
        }
        if settings.midtrans_enabled {
            registry.register(Arc::new(MidtransPaymentPlugin::from_env()));
        }
        if settings.xendit_enabled {
            registry.register(Arc::new(XenditPaymentPlugin::from_env()));
        }
        if settings.x402_enabled {
            registry.register(Arc::new(X402PaymentPlugin::from_env_with_pool(pool)));
        }

        registry.default_provider = settings
            .default_provider
            .filter(|provider| registry.plugins.contains_key(provider))
            .or_else(|| registry.names().first().cloned());

        registry
    }

    pub fn register(&mut self, plugin: Arc<dyn PaymentPlugin>) {
        self.plugins
            .insert(plugin.provider_key().to_string(), plugin);
    }

    pub fn get(&self, provider: &str) -> Option<Arc<dyn PaymentPlugin>> {
        self.plugins.get(&provider.to_ascii_lowercase()).cloned()
    }

    #[allow(dead_code)]
    pub fn default(&self) -> Option<Arc<dyn PaymentPlugin>> {
        self.default_provider
            .as_deref()
            .and_then(|provider| self.get(provider))
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
