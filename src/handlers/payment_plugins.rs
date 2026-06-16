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
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::commission;
use crate::config::Config;
use crate::payment_settings::load_payment_settings;
use crate::plugins::payment::{
    models::{ConfirmPaymentRequest, CreateInvoiceRequest, PaymentStatus},
    PaymentPluginRegistry,
};
use crate::sessions;

#[derive(Clone)]
pub struct PaymentPluginState {
    pub pool: sqlx::PgPool,
    pub cfg: Config,
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
    pub affiliate_ref: Option<String>,
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

async fn runtime_registry(state: &PaymentPluginState) -> PaymentPluginRegistry {
    PaymentPluginRegistry::from_runtime_with_pool(state.pool.clone()).await
}

fn registry_for_existing_invoices(state: &PaymentPluginState) -> PaymentPluginRegistry {
    PaymentPluginRegistry::from_all_env_known_with_pool(Some(state.pool.clone()))
}

// ---------------------------------------------------------------------------
// List providers
// ---------------------------------------------------------------------------

pub async fn list_payment_plugins(State(state): State<PaymentPluginState>) -> impl IntoResponse {
    let settings = load_payment_settings(&state.pool).await;
    let registry = runtime_registry(&state).await;
    let providers =
        PaymentPluginRegistry::capabilities_from_env_with_pool(Some(state.pool.clone()))
            .into_iter()
            .map(|capability| {
                let provider_key = capability.provider.clone();
                json!({
                    "provider": capability.provider,
                    "display_name": capability.display_name,
                    "configured": capability.configured,
                    "environment": capability.environment,
                    "api_base_url": capability.api_base_url,
                    "supports_redirect_checkout": capability.supports_redirect_checkout,
                    "supports_webhook_confirmation": capability.supports_webhook_confirmation,
                    "supports_manual_confirmation": capability.supports_manual_confirmation,
                    "supported_currencies": capability.supported_currencies,
                    "required_env": capability.required_env,
                    "missing_env": capability.missing_env,
                    "enabled": settings.is_provider_enabled(&provider_key),
                })
            })
            .collect::<Vec<_>>();

    Json(json!({
        "ok": true,
        "default_provider": registry.default_provider_name(),
        "providers": providers
    }))
}

// ---------------------------------------------------------------------------
// Create invoice
// ---------------------------------------------------------------------------

pub async fn create_default_payment_invoice(
    State(state): State<PaymentPluginState>,
    cookies: Cookies,
    Json(payload): Json<CreateInvoicePayload>,
) -> impl IntoResponse {
    let registry = runtime_registry(&state).await;
    let Some(provider) = registry.default_provider_name() else {
        return Json(json!({"ok": false, "error": "default payment provider is not configured"}));
    };
    create_invoice_with_provider(state, registry, provider, payload, cookies).await
}

pub async fn create_payment_invoice(
    State(state): State<PaymentPluginState>,
    cookies: Cookies,
    Path(provider): Path<String>,
    Json(payload): Json<CreateInvoicePayload>,
) -> impl IntoResponse {
    let registry = runtime_registry(&state).await;
    create_invoice_with_provider(state, registry, provider, payload, cookies).await
}

async fn create_invoice_with_provider(
    state: PaymentPluginState,
    registry: PaymentPluginRegistry,
    provider: String,
    payload: CreateInvoicePayload,
    cookies: Cookies,
) -> Json<serde_json::Value> {
    let Some(plugin) = registry.get(&provider) else {
        return Json(
            json!({"ok": false, "error": format!("payment provider not found: {provider}")}),
        );
    };

    // Always derive the buyer from the authenticated session. The incoming JSON
    // still carries legacy fields such as `user_id`, but the server does not
    // trust them for authorization or billing decisions.
    let Some((buyer_id, _)) = sessions::current_user_id(&state.pool, &state.cfg, &cookies).await
    else {
        return Json(json!({"ok": false, "error": "not logged in"}));
    };

    // Load the authoritative price and ownership data from the database so the
    // client cannot tamper with invoice totals or buy its own content.
    let video_row = sqlx::query!(
        "SELECT title, owner_id AS creator_id, price_cents FROM videos WHERE id = $1",
        payload.video_id
    )
    .fetch_optional(&state.pool)
    .await;

    let (video_title, creator_id, video_price_cents) = match video_row {
        Ok(Some(r)) => (
            r.title.unwrap_or_else(|| "Video".into()),
            r.creator_id,
            r.price_cents.unwrap_or(0),
        ),
        Ok(None) => return Json(json!({"ok": false, "error": "video not found"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db error: {e}")})),
    };

    if creator_id == buyer_id {
        return Json(json!({"ok": false, "error": "owners cannot buy their own video"}));
    }
    if video_price_cents < 0 {
        return Json(json!({"ok": false, "error": "invalid video price"}));
    }

    let buyer_email_from_db: Option<String> =
        sqlx::query_scalar("SELECT email FROM users WHERE id = $1 LIMIT 1")
            .bind(&buyer_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

    // Persist a local invoice first so webhook reconciliation and audit trails
    // always have an internal identifier, even before the provider responds.
    let invoice_uid = Uuid::new_v4().to_string();

    let insert_result = sqlx::query!(
        r#"INSERT INTO fiat_invoices
           (invoice_uid, provider, user_id, video_id, creator_id, amount, currency, buyer_email)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        invoice_uid,
        provider,
        buyer_id,
        payload.video_id,
        creator_id,
        video_price_cents,
        payload.currency,
        buyer_email_from_db.as_deref().unwrap_or(""),
    )
    .execute(&state.pool)
    .await;

    if let Err(e) = insert_result {
        return Json(json!({"ok": false, "error": format!("db insert error: {e}")}));
    }

    if let Some(ref_username) = payload.affiliate_ref.as_deref().filter(|s| !s.is_empty()) {
        let _ = sqlx::query("UPDATE fiat_invoices SET affiliate_ref = $1 WHERE invoice_uid = $2")
            .bind(ref_username)
            .bind(&invoice_uid)
            .execute(&state.pool)
            .await;
    }

    // Augment provider metadata with server-controlled values. This keeps later
    // reconciliation tied to our own invoice and video records.
    let mut metadata = payload.metadata;
    metadata.insert("invoice_uid".into(), invoice_uid.clone());
    metadata.insert("video_title".into(), video_title);

    let request = CreateInvoiceRequest {
        user_id: buyer_id,
        video_id: payload.video_id,
        amount_cents: video_price_cents,
        currency: payload.currency,
        buyer_email: buyer_email_from_db.or(payload.buyer_email),
        buyer_name: payload.buyer_name,
        success_url: payload.success_url,
        cancel_url: payload.cancel_url,
        metadata,
    };

    match plugin.create_invoice(request).await {
        Ok(invoice) => {
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
// Manual confirm
// ---------------------------------------------------------------------------

pub async fn confirm_default_payment(
    State(state): State<PaymentPluginState>,
    Json(payload): Json<ConfirmPaymentPayload>,
) -> impl IntoResponse {
    let registry = runtime_registry(&state).await;
    let Some(provider) = registry.default_provider_name() else {
        return Json(json!({"ok": false, "error": "default payment provider is not configured"}));
    };
    confirm_payment_with_provider(state, registry, provider, payload).await
}

pub async fn confirm_payment(
    State(state): State<PaymentPluginState>,
    Path(provider): Path<String>,
    Json(payload): Json<ConfirmPaymentPayload>,
) -> impl IntoResponse {
    let registry = registry_for_existing_invoices(&state);
    confirm_payment_with_provider(state, registry, provider, payload).await
}

async fn confirm_payment_with_provider(
    state: PaymentPluginState,
    registry: PaymentPluginRegistry,
    provider: String,
    payload: ConfirmPaymentPayload,
) -> Json<serde_json::Value> {
    let Some(plugin) = registry.get(&provider) else {
        return Json(
            json!({"ok": false, "error": format!("payment provider not found: {provider}")}),
        );
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

// ---------------------------------------------------------------------------
// Webhook handler
// ---------------------------------------------------------------------------

pub async fn handle_webhook(
    State(state): State<PaymentPluginState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let registry = registry_for_existing_invoices(&state);
    let Some(plugin) = registry.get(&provider) else {
        return Json(
            json!({"ok": false, "error": format!("payment provider not found: {provider}")}),
        );
    };

    let raw_b64 = B64.encode(&body);
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let mut payload = parsed;
    payload["__raw__"] = json!(raw_b64);

    let mut sig_headers = std::collections::HashMap::<String, String>::new();
    for (k, v) in headers.iter() {
        if let Ok(val) = v.to_str() {
            sig_headers.insert(k.as_str().to_lowercase(), val.to_string());
        }
    }

    let request = ConfirmPaymentRequest {
        provider: provider.clone(),
        invoice_id: String::new(),
        transaction_id: None,
        webhook_payload: Some(payload),
        signature_headers: sig_headers,
    };

    // Delegate signature verification and payload interpretation to the
    // provider plugin, then apply our own local idempotency and entitlement
    // rules to the normalized result.
    let result = match plugin.confirm_payment(request).await {
        Ok(r) => r,
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

    let invoice_uid = &result.invoice_id;

    let inv = sqlx::query!(
        r#"SELECT fi.user_id, fi.video_id, fi.creator_id, fi.amount, fi.currency,
                  fi.status, fi.paid_at, fi.disbursed_at,
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
        Ok(None) => {
            tracing::warn!("webhook: no fiat_invoice row for uid={invoice_uid}");
            return Json(json!({"ok": false, "error": "invoice not found"}));
        }
        Err(e) => {
            tracing::error!("webhook: db fetch error: {e}");
            return Json(json!({"ok": false, "error": "db error"}));
        }
    };

    // Ignore replayed webhooks once the invoice has already been finalized as
    // paid. This prevents duplicate entitlement grants and disbursement logic.
    if inv.status == "paid" && inv.paid_at.is_some() {
        tracing::info!(
            "fiat webhook replay ignored: provider={provider} uid={invoice_uid} already paid"
        );
        return Json(json!({
            "ok": true,
            "status": "paid",
            "invoice_id": invoice_uid,
            "video_id": inv.video_id,
            "replayed": true
        }));
    }

    let username = inv.buyer_username.clone();

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

    let _ = sqlx::query!(
        r#"INSERT INTO purchases (user_id, video_id, created_at)
           VALUES ($1, $2, NOW())
           ON CONFLICT DO NOTHING"#,
        inv.user_id,
        inv.video_id,
    )
    .execute(&state.pool)
    .await;

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
        inv.user_id,
        inv.video_id
    );

    let aff_ref: Option<String> =
        sqlx::query("SELECT affiliate_ref FROM fiat_invoices WHERE invoice_uid = $1 LIMIT 1")
            .bind(invoice_uid)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .and_then(|r: sqlx::postgres::PgRow| {
                use sqlx::Row as _;
                r.try_get::<Option<String>, _>("affiliate_ref")
                    .ok()
                    .flatten()
            });

    if let Some(ref_username) = aff_ref.as_deref().filter(|s| !s.is_empty()) {
        if let Err(e) = commission::process_affiliate_commission(
            &state.pool,
            &inv.video_id,
            &inv.user_id,
            &inv.creator_id,
            inv.amount,
            ref_username,
            &provider,
            Some(invoice_uid),
        )
        .await
        {
            tracing::warn!("fiat affiliate commission skipped: {e}");
        }
    }

    // Auto-disburse only once for providers that support it natively. A replay
    // or repeated callback should not produce multiple creator payouts.
    if provider == "xendit" && inv.disbursed_at.is_none() {
        use crate::plugins::payment::providers::xendit::XenditPaymentPlugin;

        if let Some(ba) = inv.creator_bank {
            if !ba.trim().is_empty() {
                let xp = XenditPaymentPlugin::from_env();
                match xp.disburse_to_creator(&ba, inv.amount, invoice_uid).await {
                    Ok(disburse_resp) => {
                        let disburse_ref = disburse_resp["id"].as_str().unwrap_or("").to_string();
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
    }

    Json(json!({
        "ok": true,
        "status": "paid",
        "invoice_id": invoice_uid,
        "video_id": inv.video_id
    }))
}
