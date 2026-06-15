// src/handlers/payment_plugins.rs

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest},
    PaymentPluginRegistry,
};

#[derive(Clone)]
pub struct PaymentPluginState {
    pub registry: PaymentPluginRegistry,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoicePayload {
    pub user_id: String,
    pub video_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub buyer_email: Option<String>,
    pub buyer_name: Option<String>,
    pub success_url: Option<String>,
    pub cancel_url: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmPaymentPayload {
    pub invoice_id: String,
    pub transaction_id: Option<String>,
    pub provider_payload: Option<serde_json::Value>,
    #[serde(default)]
    pub signature_headers: std::collections::HashMap<String, String>,
}

pub async fn list_payment_plugins(
    State(state): State<PaymentPluginState>,
) -> impl IntoResponse {
    let providers = state
        .registry
        .names()
        .into_iter()
        .filter_map(|name| state.registry.get(&name))
        .map(|plugin| plugin.capability())
        .collect::<Vec<_>>();

    Json(json!({
        "ok": true,
        "default_provider": state.registry.default_provider_name(),
        "providers": providers
    }))
}

pub async fn create_default_payment_invoice(
    State(state): State<PaymentPluginState>,
    Json(payload): Json<CreateInvoicePayload>,
) -> impl IntoResponse {
    let Some(provider) = state.registry.default_provider_name() else {
        return Json(json!({"ok": false, "error": "default payment provider is not configured"}));
    };

    create_invoice_with_provider(state, provider, payload).await
}

pub async fn create_payment_invoice(
    State(state): State<PaymentPluginState>,
    Path(provider): Path<String>,
    Json(payload): Json<CreateInvoicePayload>,
) -> impl IntoResponse {
    create_invoice_with_provider(state, provider, payload).await
}

async fn create_invoice_with_provider(
    state: PaymentPluginState,
    provider: String,
    payload: CreateInvoicePayload,
) -> Json<serde_json::Value> {
    let Some(plugin) = state.registry.get(&provider) else {
        return Json(json!({"ok": false, "error": format!("payment provider not found: {provider}")}));
    };

    let request = CreateInvoiceRequest {
        user_id: payload.user_id,
        video_id: payload.video_id,
        amount_cents: payload.amount_cents,
        currency: payload.currency,
        buyer_email: payload.buyer_email,
        buyer_name: payload.buyer_name,
        success_url: payload.success_url,
        cancel_url: payload.cancel_url,
        metadata: payload.metadata,
    };

    match plugin.create_invoice(request).await {
        Ok(invoice) => Json(json!({"ok": true, "provider": provider, "invoice": invoice})),
        Err(e) => Json(json!({"ok": false, "provider": provider, "error": e.to_string()})),
    }
}

pub async fn confirm_default_payment(
    State(state): State<PaymentPluginState>,
    Json(payload): Json<ConfirmPaymentPayload>,
) -> impl IntoResponse {
    let Some(provider) = state.registry.default_provider_name() else {
        return Json(json!({"ok": false, "error": "default payment provider is not configured"}));
    };

    confirm_payment_with_provider(state, provider, payload).await
}

pub async fn confirm_payment(
    State(state): State<PaymentPluginState>,
    Path(provider): Path<String>,
    Json(payload): Json<ConfirmPaymentPayload>,
) -> impl IntoResponse {
    confirm_payment_with_provider(state, provider, payload).await
}

async fn confirm_payment_with_provider(
    state: PaymentPluginState,
    provider: String,
    payload: ConfirmPaymentPayload,
) -> Json<serde_json::Value> {
    let Some(plugin) = state.registry.get(&provider) else {
        return Json(json!({"ok": false, "error": format!("payment provider not found: {provider}")}));
    };

    let request = ConfirmPaymentRequest {
        provider: provider.clone(),
        invoice_id: payload.invoice_id,
        transaction_id: payload.transaction_id,
        webhook_payload: payload.provider_payload,
        signature_headers: payload.signature_headers,
    };

    match plugin.confirm_payment(request).await {
        Ok(result) => Json(json!({"ok": true, "provider": provider, "payment": result})),
        Err(e) => Json(json!({"ok": false, "provider": provider, "error": e.to_string()})),
    }
}
