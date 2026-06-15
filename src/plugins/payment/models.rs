// src/plugins/payment/models.rs
//
// Shared provider-neutral payment data models.
//
// HTTP handlers and business services should depend on these models instead of
// provider-specific request or response formats. Each plugin is responsible for
// translating these neutral models into the provider API contract.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateInvoiceRequest {
    pub user_id: String,
    pub video_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub buyer_email: Option<String>,
    pub buyer_name: Option<String>,
    pub success_url: Option<String>,
    pub cancel_url: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invoice {
    pub provider: String,
    pub invoice_id: String,
    pub payment_url: Option<String>,
    pub amount_cents: i64,
    pub currency: String,
    pub status: PaymentStatus,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmPaymentRequest {
    pub provider: String,
    pub invoice_id: String,
    pub transaction_id: Option<String>,
    pub webhook_payload: Option<serde_json::Value>,
    pub signature_headers: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentResult {
    pub provider: String,
    pub invoice_id: String,
    pub transaction_id: Option<String>,
    pub status: PaymentStatus,
    pub paid_amount_cents: i64,
    pub currency: String,
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,
    Paid,
    Failed,
    Expired,
    Cancelled,
    Underpaid,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaymentPluginCapability {
    pub provider: String,
    pub display_name: String,
    pub configured: bool,
    pub environment: String,
    pub api_base_url: Option<String>,
    pub supports_redirect_checkout: bool,
    pub supports_webhook_confirmation: bool,
    pub supports_manual_confirmation: bool,
    pub supported_currencies: Vec<String>,
    pub required_env: Vec<String>,
    pub missing_env: Vec<String>,
}
