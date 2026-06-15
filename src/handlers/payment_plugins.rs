// src/handlers/payment_plugins.rs

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::config::Config;
use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, PaymentStatus},
    PaymentPluginRegistry,
};

#[derive(Clone)]
pub struct PaymentPluginState {
    pub registry: PaymentPluginRegistry,
    pub pool:     sqlx::PgPool,
    pub cfg:      Config,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoicePayload {
    pub user_id:       String,
    pub video_id:      String,
    pub amount_cents:  i64,
    pub currency:      String,
    pub buyer_email:   Option<String>,
    pub buyer_name:    Option<String>,
    pub success_url:   Option<String>,
    pub cancel_url:    Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfirmPaymentPayload {
    pub invoice_id:        String,
    pub transaction_id:    Option<String>,
    pub provider_payload:  Option<serde_json::Value>,
    #[serde(default)]
    pub signature_headers: std::collections::HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// List providers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Create invoice
// ---------------------------------------------------------------------------

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
    state:    PaymentPluginState,
    provider: String,
    payload:  CreateInvoicePayload,
) -> Json<serde_json::Value> {
    let Some(plugin) = state.registry.get(&provider) else {
        return Json(json!({"ok": false, "error": format!("payment provider not found: {provider}")}));
    };

    // Fetch video info (title + creator_id) for the fiat_invoices row
    let video_row = sqlx::query!(
        "SELECT title, owner_id AS creator_id FROM videos WHERE id = $1",
        payload.video_id
    )
    .fetch_optional(&state.pool)
    .await;

    let (video_title, creator_id) = match video_row {
        Ok(Some(r)) => (r.title.unwrap_or_else(|| "Video".into()), r.creator_id),
        Ok(None)    => return Json(json!({"ok": false, "error": "video not found"})),
        Err(e)      => return Json(json!({"ok": false, "error": format!("db error: {e}")})),
    };

    let invoice_uid = Uuid::new_v4().to_string();

    // Pre-insert the invoice so we can update it with provider_ref + payment_url after creation
    let insert_result = sqlx::query!(
        r#"INSERT INTO fiat_invoices
           (invoice_uid, provider, user_id, video_id, creator_id, amount, currency, buyer_email)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        invoice_uid,
        provider,
        payload.user_id,
        payload.video_id,
        creator_id,
        payload.amount_cents,
        payload.currency,
        payload.buyer_email.as_deref().unwrap_or(""),
    )
    .execute(&state.pool)
    .await;

    if let Err(e) = insert_result {
        return Json(json!({"ok": false, "error": format!("db insert error: {e}")}));
    }

    // Inject uid and title into metadata so the plugin can embed them in provider requests
    let mut metadata = payload.metadata;
    metadata.insert("invoice_uid".into(), invoice_uid.clone());
    metadata.insert("video_title".into(), video_title);

    let request = CreateInvoiceRequest {
        user_id:      payload.user_id,
        video_id:     payload.video_id,
        amount_cents: payload.amount_cents,
        currency:     payload.currency,
        buyer_email:  payload.buyer_email,
        buyer_name:   payload.buyer_name,
        success_url:  payload.success_url,
        cancel_url:   payload.cancel_url,
        metadata,
    };

    match plugin.create_invoice(request).await {
        Ok(invoice) => {
            // Update fiat_invoices with the provider's reference ID and redirect URL
            let provider_ref = invoice.raw["order_id"]
                .as_str()
                .or_else(|| invoice.raw["xendit_invoice_id"].as_str())
                .or_else(|| invoice.raw["session_id"].as_str())
                .unwrap_or(&invoice_uid);

            let _ = sqlx::query!(
                "UPDATE fiat_invoices SET provider_ref = $1, payment_url = $2 WHERE invoice_uid = $3",
                provider_ref,
                invoice.payment_url.as_deref(),
                invoice_uid,
            )
            .execute(&state.pool)
            .await;

            Json(json!({"ok": true, "provider": provider, "invoice": invoice}))
        }
        Err(e) => {
            // Clean up the pending row if the provider call failed
            let _ = sqlx::query!(
                "DELETE FROM fiat_invoices WHERE invoice_uid = $1",
                invoice_uid
            )
            .execute(&state.pool)
            .await;
            Json(json!({"ok": false, "provider": provider, "error": e.to_string()}))
        }
    }
}

// ---------------------------------------------------------------------------
// Manual confirm (frontend polling — only for providers that support it)
// ---------------------------------------------------------------------------

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
    state:    PaymentPluginState,
    provider: String,
    payload:  ConfirmPaymentPayload,
) -> Json<serde_json::Value> {
    let Some(plugin) = state.registry.get(&provider) else {
        return Json(json!({"ok": false, "error": format!("payment provider not found: {provider}")}));
    };

    let request = ConfirmPaymentRequest {
        provider:          provider.clone(),
        invoice_id:        payload.invoice_id,
        transaction_id:    payload.transaction_id,
        webhook_payload:   payload.provider_payload,
        signature_headers: payload.signature_headers,
    };

    match plugin.confirm_payment(request).await {
        Ok(result) => Json(json!({"ok": true, "provider": provider, "payment": result})),
        Err(e)     => Json(json!({"ok": false, "provider": provider, "error": e.to_string()})),
    }
}

// ---------------------------------------------------------------------------
// Webhook handler — called by payment providers, not by the frontend
// ---------------------------------------------------------------------------

pub async fn handle_webhook(
    State(state):  State<PaymentPluginState>,
    Path(provider): Path<String>,
    headers:       HeaderMap,
    body:          Bytes,
) -> impl IntoResponse {
    let Some(plugin) = state.registry.get(&provider) else {
        return Json(json!({"ok": false, "error": format!("payment provider not found: {provider}")}));
    };

    // Encode raw bytes as base64 so Stripe's plugin can HMAC-verify the exact bytes received
    let raw_b64  = B64.encode(&body);
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let mut payload = parsed;
    payload["__raw__"] = json!(raw_b64);

    // Collect headers into a lowercase HashMap for uniform access in plugins
    let mut sig_headers = std::collections::HashMap::<String, String>::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            sig_headers.insert(k.as_str().to_lowercase(), val.to_string());
        }
    }

    let request = ConfirmPaymentRequest {
        provider:          provider.clone(),
        invoice_id:        String::new(), // determined from payload by the plugin
        transaction_id:    None,
        webhook_payload:   Some(payload),
        signature_headers: sig_headers,
    };

    let result = match plugin.confirm_payment(request).await {
        Ok(r)  => r,
        Err(e) => {
            tracing::warn!("webhook confirm_payment error ({provider}): {e}");
            return Json(json!({"ok": false, "error": e.to_string()}));
        }
    };

    if result.status != PaymentStatus::Paid {
        return Json(json!({
            "ok": true,
            "status": format!("{:?}", result.status),
            "invoice_id": result.invoice_id
        }));
    }

    // ---- Grant access ----
    let invoice_uid = &result.invoice_id;

    // Single JOIN fetches invoice + buyer username + creator bank_account in one round trip
    let inv = sqlx::query!(
        r#"SELECT fi.user_id, fi.video_id, fi.creator_id, fi.amount, fi.currency,
                  buyer.username  AS buyer_username,
                  creator.bank_account AS creator_bank
           FROM fiat_invoices fi
           JOIN users buyer   ON buyer.id   = fi.user_id
           JOIN users creator ON creator.id = fi.creator_id
           WHERE fi.invoice_uid = $1"#,
        invoice_uid
    )
    .fetch_optional(&state.pool)
    .await;

    let inv = match inv {
        Ok(Some(r)) => r,
        Ok(None)    => {
            tracing::warn!("webhook: no fiat_invoice row for uid={invoice_uid}");
            return Json(json!({"ok": false, "error": "invoice not found"}));
        }
        Err(e) => {
            tracing::error!("webhook: db fetch error: {e}");
            return Json(json!({"ok": false, "error": "db error"}));
        }
    };

    let username = inv.buyer_username.clone();

    // Mark invoice paid
    let _ = sqlx::query!(
        r#"UPDATE fiat_invoices
           SET status = 'paid', paid_at = now(),
               provider_ref = COALESCE(provider_ref, $2)
           WHERE invoice_uid = $1"#,
        invoice_uid,
        result.transaction_id.as_deref().unwrap_or(""),
    )
    .execute(&state.pool)
    .await;

    // Insert purchase record
    let _ = sqlx::query!(
        r#"INSERT INTO purchases (user_id, video_id, created_at)
           VALUES ($1, $2, NOW())
           ON CONFLICT DO NOTHING"#,
        inv.user_id,
        inv.video_id,
    )
    .execute(&state.pool)
    .await;

    // Grant streaming access
    let _ = sqlx::query!(
        r#"INSERT INTO allowlist (video_id, username)
           VALUES ($1, $2)
           ON CONFLICT (video_id, username) DO NOTHING"#,
        inv.video_id,
        username,
    )
    .execute(&state.pool)
    .await;

    tracing::info!(
        "fiat payment granted: provider={provider} uid={invoice_uid} user={} video={}",
        inv.user_id, inv.video_id
    );

    // ---- Xendit auto-disburse ----
    // Only possible for Xendit and only when the creator has a bank_account set.
    // Instantiate a fresh plugin from env to access the disburse_to_creator() method
    // (avoid trait-object downcast complexity).
    if provider == "xendit" {
        use crate::plugins::payment::providers::xendit::XenditPaymentPlugin;

        if let Some(ba) = inv.creator_bank {
            if !ba.trim().is_empty() {
                    let xp = XenditPaymentPlugin::from_env();
                    match xp.disburse_to_creator(&ba, inv.amount, invoice_uid).await {
                        Ok(disburse_resp) => {
                            let disburse_ref = disburse_resp["id"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            let _ = sqlx::query!(
                                "UPDATE fiat_invoices SET disbursed_at = now(), disburse_ref = $1 WHERE invoice_uid = $2",
                                disburse_ref,
                                invoice_uid,
                            )
                            .execute(&state.pool)
                            .await;
                            tracing::info!("xendit: disbursed {disburse_ref} for uid={invoice_uid}");
                        }
                        Err(e) => {
                            tracing::error!("xendit: disburse failed for uid={invoice_uid}: {e}");
                        }
                    }
            }
        }
    }  // if provider == "xendit"

    Json(json!({
        "ok": true,
        "status": "paid",
        "invoice_id": invoice_uid,
        "video_id": inv.video_id
    }))
}
