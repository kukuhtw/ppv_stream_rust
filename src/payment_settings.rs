use serde::Serialize;
use sqlx::{PgPool, Row};

#[derive(Clone, Debug, Serialize)]
pub struct PaymentSettings {
    pub wallet_payment_enabled: bool,
    pub wallet_transfer_enabled: bool,
    pub paypal_enabled: bool,
    pub stripe_enabled: bool,
    pub xendit_enabled: bool,
    pub midtrans_enabled: bool,
    pub x402_enabled: bool,
    pub default_provider: Option<String>,
}

impl Default for PaymentSettings {
    fn default() -> Self {
        Self {
            wallet_payment_enabled: true,
            wallet_transfer_enabled: true,
            paypal_enabled: true,
            stripe_enabled: true,
            xendit_enabled: true,
            midtrans_enabled: true,
            x402_enabled: true,
            default_provider: None,
        }
    }
}

impl PaymentSettings {
    pub fn is_provider_enabled(&self, provider: &str) -> bool {
        match provider.trim().to_ascii_lowercase().as_str() {
            "paypal" => self.paypal_enabled,
            "stripe" => self.stripe_enabled,
            "xendit" => self.xendit_enabled,
            "midtrans" => self.midtrans_enabled,
            "x402" => self.x402_enabled,
            _ => false,
        }
    }
}

pub async fn load_payment_settings(pool: &PgPool) -> PaymentSettings {
    let row = sqlx::query(
        r#"SELECT wallet_payment_enabled, wallet_transfer_enabled,
                  paypal_enabled, stripe_enabled, xendit_enabled, midtrans_enabled,
                  x402_enabled, default_provider
           FROM payment_settings
           WHERE id = TRUE
           LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some(r)) => PaymentSettings {
            wallet_payment_enabled: r.try_get("wallet_payment_enabled").unwrap_or(true),
            wallet_transfer_enabled: r.try_get("wallet_transfer_enabled").unwrap_or(true),
            paypal_enabled: r.try_get("paypal_enabled").unwrap_or(true),
            stripe_enabled: r.try_get("stripe_enabled").unwrap_or(true),
            xendit_enabled: r.try_get("xendit_enabled").unwrap_or(true),
            midtrans_enabled: r.try_get("midtrans_enabled").unwrap_or(true),
            x402_enabled: r.try_get("x402_enabled").unwrap_or(true),
            default_provider: r
                .try_get::<Option<String>, _>("default_provider")
                .unwrap_or(None)
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty()),
        },
        _ => PaymentSettings::default(),
    }
}
