// src/handlers/pay.rs
//
// Payment option, x402 invoice, cryptocurrency price, and payment confirmation handlers.
//
// This module is responsible for:
// 1. Returning the payment methods supported for a video.
// 2. Creating an x402 invoice and signing the payment authorization payload.
// 3. Fetching and caching cryptocurrency market prices.
// 4. Verifying an EVM transaction receipt and decoding the Paid smart contract event.
// 5. Marking invoices as paid or underpaid.
// 6. Unlocking purchased video access through purchases and allowlist records.

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use ethabi::{Event, EventParam, ParamType, RawLog, Token as EventToken};
use ethereum_types::H256;
use ethers::abi::{encode as abi_encode, Token as AbiToken};
use ethers::core::utils::keccak256;
use ethers::signers::LocalWallet;
use ethers::types::{Address, Signature, U256};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::types::BigDecimal;
use sqlx::Row;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::commission;
use crate::handlers::video::VideoState;
use crate::payment_settings::load_payment_settings;
use crate::sessions;

/// Returns the available payment configuration for one video.
///
/// Route: `GET /api/pay/options?video_id=<id>`
///
/// The response contains the video price, creator wallet information, preferred
/// creator chain, and all active token configurations from `pay_tokens`.
pub async fn pay_options(
    State(st): State<VideoState>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    // Read the required video ID from the query string.
    let video_id = query.get("video_id").cloned().unwrap_or_default();
    if video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    // Load the requested video together with the creator wallet settings.
    let row = match sqlx::query(
        r#"
        SELECT v.id, v.price_cents, v.owner_id, u.wallet_account, u.wallet_chain_id
        FROM videos v
        JOIN users u ON u.id = v.owner_id
        WHERE v.id = $1
        LIMIT 1
        "#,
    )
    .bind(&video_id)
    .fetch_optional(&st.pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            return Json(json!({"ok": false, "error": format!("db error: {e}")}));
        }
    };

    let Some(video_metadata) = row else {
        return Json(json!({"ok": false, "error": "video not found"}));
    };

    // Load every active payment token. COALESCE supports both the current
    // `erc20_address` column and the legacy `erc20` column.
    let tokens = sqlx::query(
        r#"
        SELECT chain, chain_id, symbol, decimals,
               COALESCE(erc20_address, erc20) AS erc20_address
        FROM pay_tokens
        WHERE is_active = TRUE
        ORDER BY chain_id, symbol
        "#,
    )
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    // Convert database rows into the frontend payment option response.
    Json(json!({
        "ok": true,
        "video_id": video_id,
        "price_cents": video_metadata.try_get::<i64, _>("price_cents").unwrap_or(0),
        "creator_id": video_metadata
            .try_get::<String, _>("owner_id")
            .unwrap_or_default(),
        "creator_wallet": video_metadata
            .try_get::<Option<String>, _>("wallet_account")
            .ok()
            .flatten(),
        "creator_chain_id": video_metadata
            .try_get::<Option<i64>, _>("wallet_chain_id")
            .ok()
            .flatten(),
        "tokens": tokens
            .iter()
            .map(|token| {
                json!({
                    "chain": token.try_get::<String, _>("chain").unwrap_or_default(),
                    "chain_id": token.try_get::<i64, _>("chain_id").unwrap_or_default(),
                    "symbol": token.try_get::<String, _>("symbol").unwrap_or_default(),
                    "decimals": token.try_get::<i32, _>("decimals").unwrap_or_default(),
                    "erc20": token
                        .try_get::<Option<String>, _>("erc20_address")
                        .ok()
                        .flatten(),
                })
            })
            .collect::<Vec<_>>()
    }))
}

/// JSON request body accepted by `POST /api/pay/x402/start`.
#[derive(Deserialize)]
pub struct StartPayReq {
    pub video_id: String,
    pub chain_id: i64,
    pub symbol: String,
    pub token_address: Option<String>,
    pub payer_address: String,
    pub ref_code: Option<String>, // affiliate referral username
}

/// JSON response returned after an x402 payment invoice is created and signed.
#[derive(Serialize)]
pub struct StartPayResp {
    pub ok: bool,
    pub invoice_uid: String,
    pub invoice_uid_bytes32: String,
    pub video_id: String,
    pub chain_id: i64,
    pub symbol: String,
    pub token_address: Option<String>,
    pub amount_wei: String,
    pub min_amount_wei: String,
    pub deadline: u64,
    pub v: u8,
    pub r: String,
    pub s: String,
    pub split_creator_bp: i32,
    pub split_admin_bp: i32,
    pub x402_contract: String,
    pub creator_wallet: String,
}

/// Creates a pending x402 invoice and signs the smart contract authorization payload.
///
/// Route: `POST /api/pay/x402/start`
///
/// Flow:
/// 1. Authenticate the buyer.
/// 2. Validate the payer address and payment parameters.
/// 3. Load video, creator wallet, and token metadata.
/// 4. Convert the configured price into token base units.
/// 5. Create and persist a pending invoice.
/// 6. ABI encode and hash the contract verification payload.
/// 7. Sign the payload using the administrative signing key.
/// 8. Return invoice and signature fields to the frontend.
pub async fn x402_start(
    State(st): State<VideoState>,
    cookies: Cookies,
    Json(body): Json<StartPayReq>,
) -> impl IntoResponse {
    let payment_settings = load_payment_settings(&st.pool).await;
    if !payment_settings.x402_enabled {
        return Json(json!({"ok": false, "error": "x402 payment is currently disabled by admin"}));
    }

    // Authenticate the buyer from the signed session cookie.
    let (buyer_id, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await {
        Some(value) => value,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    // Validate mandatory payment fields before querying the database.
    if body.video_id.is_empty() || body.chain_id == 0 || body.symbol.is_empty() {
        return Json(json!({"ok": false, "error": "invalid params"}));
    }

    // Parse and validate the buyer wallet as an EVM address. The payer address
    // becomes part of the signed smart contract authorization payload.
    let payer_address = match Address::from_str(&body.payer_address) {
        Ok(address) => address,
        Err(_) => return Json(json!({"ok": false, "error": "invalid payer_address"})),
    };

    // Load the video price, creator ID, and creator wallet.
    let video_metadata = sqlx::query(
        r#"
        SELECT v.price_cents, v.owner_id, u.wallet_account
        FROM videos v
        JOIN users u ON u.id = v.owner_id
        WHERE v.id = $1
        LIMIT 1
        "#,
    )
    .bind(&body.video_id)
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);

    let Some(video_metadata) = video_metadata else {
        return Json(json!({"ok": false, "error": "video not found"}));
    };

    // A creator wallet is required because the smart contract splits payment
    // between the creator and platform administrator.
    let creator_wallet_string = match video_metadata.try_get::<Option<String>, _>("wallet_account")
    {
        Ok(Some(wallet)) if !wallet.is_empty() => wallet,
        _ => return Json(json!({"ok": false, "error": "creator has no wallet"})),
    };

    let creator_address = match Address::from_str(&creator_wallet_string) {
        Ok(address) => address,
        Err(_) => return Json(json!({"ok": false, "error": "invalid creator wallet"})),
    };

    // Validate that the requested token configuration is active and matches
    // the selected chain, symbol, and optional token contract address.
    let token_info = sqlx::query(
        r#"
        SELECT decimals,
               COALESCE(erc20_address, erc20) AS erc20_address
        FROM pay_tokens
        WHERE chain_id=$1 AND symbol=$2
          AND COALESCE(erc20_address, erc20, '') = COALESCE($3, '')
          AND is_active=TRUE
        LIMIT 1
        "#,
    )
    .bind(body.chain_id)
    .bind(&body.symbol)
    .bind(&body.token_address)
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);

    let Some(token_info) = token_info else {
        return Json(json!({"ok": false, "error": "token not supported"}));
    };

    let decimals = token_info.try_get::<i32, _>("decimals").unwrap_or(18) as u32;
    let price_cents: i64 = video_metadata.try_get::<i64, _>("price_cents").unwrap_or(0);

    // Convert price cents into token base units and round upward. For example,
    // an 18-decimal token uses 10^18 base units per whole token.
    let token_amount_wei: u128 = if price_cents <= 0 {
        0
    } else {
        let numerator = (price_cents as u128).saturating_mul(10u128.pow(decimals));
        numerator.saturating_add(99) / 100
    };

    if token_amount_wei == 0 {
        return Json(json!({
            "ok": false,
            "error": "calculated amount is zero"
        }));
    }

    // Generate an application invoice ID and its bytes32 Keccak hash. The hash
    // is used as the indexed invoice UID in the smart contract Paid event.
    let invoice_uid = Uuid::new_v4().to_string();
    let invoice_uid_bytes32 = H256::from_slice(&keccak256(invoice_uid.as_bytes()));
    let invoice_uid_hash_hex = format!("{:#066x}", invoice_uid_bytes32);

    // PostgreSQL NUMERIC values are represented through BigDecimal so token
    // amounts are stored without floating point precision loss.
    let token_amount_decimal =
        BigDecimal::from_str(&token_amount_wei.to_string()).unwrap_or_else(|_| BigDecimal::from(0));

    // Persist a pending invoice before returning the signing payload. The stored
    // invoice is later matched against the on-chain Paid event.
    let _ = sqlx::query(
        r#"
        INSERT INTO x402_invoices
          (invoice_uid, invoice_uid_hash, user_id, video_id, creator_id,
           chain_id, token_symbol, token_address,
           price_cents, token_amount, required_amount_wei,
           status, expires_at)
        VALUES
          ($1,$2,$3,$4,$5,
           $6,$7,$8,
           $9,$10,$11,
           'pending', NOW() + INTERVAL '10 minutes')
        "#,
    )
    .bind(&invoice_uid)
    .bind(&invoice_uid_hash_hex)
    .bind(&buyer_id)
    .bind(&body.video_id)
    .bind(
        video_metadata
            .try_get::<String, _>("owner_id")
            .unwrap_or_default(),
    )
    .bind(body.chain_id)
    .bind(&body.symbol)
    .bind(&body.token_address)
    .bind(price_cents)
    .bind(&token_amount_decimal)
    .bind(&token_amount_decimal)
    .execute(&st.pool)
    .await;

    // Store affiliate_ref if provided (separate runtime query — new column)
    if let Some(ref_username) = body.ref_code.as_deref().filter(|s| !s.is_empty()) {
        let _ = sqlx::query("UPDATE x402_invoices SET affiliate_ref = $1 WHERE invoice_uid = $2")
            .bind(ref_username)
            .bind(&invoice_uid)
            .execute(&st.pool)
            .await;
    }

    // Load the target smart contract address. The frontend is expected to
    // perform an additional deployed-code validation before submitting payment.
    let x402_contract = std::env::var("X402_CONTRACT_ADDRESS").unwrap_or_default();
    let _contract_address = Address::from_str(&x402_contract).unwrap_or(Address::zero());

    // A missing token address represents payment with the native chain asset.
    let token_address = body
        .token_address
        .as_deref()
        .and_then(|value| Address::from_str(value).ok())
        .unwrap_or(Address::zero());

    let creator_basis_points = st.cfg.creator_split_bp;
    let minimum_amount_wei = U256::from_dec_str(&token_amount_wei.to_string()).unwrap();
    let deadline: u64 = (chrono::Utc::now().timestamp() as u64) + st.cfg.x402_deadline_secs;
    let video_hash = H256::from_slice(&keccak256(body.video_id.as_bytes()));

    // ABI encode values in the exact order expected by the smart contract
    // verification function.
    let encoded_payload = abi_encode(&[
        AbiToken::FixedBytes(invoice_uid_bytes32.as_bytes().to_vec()),
        AbiToken::Address(token_address),
        AbiToken::Uint(minimum_amount_wei),
        AbiToken::Address(creator_address),
        AbiToken::Uint(U256::from(creator_basis_points)),
        AbiToken::FixedBytes(video_hash.as_bytes().to_vec()),
        AbiToken::Address(payer_address),
        AbiToken::Uint(U256::from(deadline)),
    ]);

    // Hash the ABI payload and then apply the EIP-191 personal-sign prefix for
    // a 32-byte message.
    let message_hash = H256::from_slice(&keccak256(&encoded_payload));
    let ethereum_signed_hash = H256::from_slice(&keccak256(
        [b"\x19Ethereum Signed Message:\n32", message_hash.as_bytes()].concat(),
    ));

    // The administrative private key signs the authorization payload. The key
    // must only be supplied through secure runtime configuration.
    let admin_private_key = std::env::var("X402_ADMIN_PRIVKEY").unwrap_or_default();
    if admin_private_key.is_empty() {
        return Json(json!({
            "ok": false,
            "error": "X402_ADMIN_PRIVKEY not set"
        }));
    }

    let admin_wallet: LocalWallet = match admin_private_key.parse() {
        Ok(wallet) => wallet,
        Err(_) => return Json(json!({"ok": false, "error": "bad admin privkey"})),
    };

    // `sign_hash` is synchronous because the private key is available locally.
    let signature: Signature = match admin_wallet.sign_hash(ethereum_signed_hash) {
        Ok(signature) => signature,
        Err(e) => return Json(json!({"ok": false, "error": format!("sign: {e}")})),
    };

    // Return all values required by the frontend to call the smart contract.
    let response = StartPayResp {
        ok: true,
        invoice_uid,
        invoice_uid_bytes32: format!("{:#066x}", invoice_uid_bytes32),
        video_id: body.video_id,
        chain_id: body.chain_id,
        symbol: body.symbol,
        token_address: body.token_address,
        amount_wei: token_amount_wei.to_string(),
        min_amount_wei: minimum_amount_wei.to_string(),
        deadline,
        v: signature.v as u8,
        r: format!("{:#066x}", signature.r),
        s: format!("{:#066x}", signature.s),
        split_creator_bp: st.cfg.creator_split_bp as i32,
        split_admin_bp: (10000u16 - st.cfg.creator_split_bp) as i32,
        x402_contract,
        creator_wallet: creator_wallet_string,
    };

    Json(json!(response))
}

/// One in-memory cryptocurrency price cache entry.
#[derive(Clone, Debug)]
struct CacheEntry {
    /// Time at which the response was cached.
    at: Instant,
    /// Raw CoinGecko JSON response.
    json: serde_json::Value,
}

/// Process-local cryptocurrency price cache protected by a mutex.
static PRICE_CACHE: Lazy<Mutex<HashMap<String, CacheEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Query parameters accepted by `GET /api/crypto_price`.
#[derive(Deserialize)]
pub struct PriceQs {
    /// Comma-separated CoinGecko asset IDs.
    pub ids: String,
    /// Optional comma-separated quote currencies. Defaults to IDR.
    pub vs: Option<String>,
}

/// Fetches cryptocurrency prices from CoinGecko with a sixty-second memory cache.
///
/// Route example:
/// `GET /api/crypto_price?ids=ethereum,usd-coin&vs=idr,usd`
pub async fn crypto_price(Query(query): Query<PriceQs>) -> impl IntoResponse {
    let ids = query.ids.trim();
    if ids.is_empty() {
        return Json(json!({"ok": false, "error": "ids required"}));
    }

    let quote_currencies = query.vs.unwrap_or_else(|| "idr".to_string());
    let cache_key = format!("{}|{}", ids, quote_currencies);

    // Return a cached response while it is younger than sixty seconds.
    if let Some(cache_hit) = PRICE_CACHE.lock().unwrap().get(&cache_key) {
        if cache_hit.at.elapsed() < Duration::from_secs(60) {
            return Json(json!({"ok": true, "data": cache_hit.json}));
        }
    }

    // Build the CoinGecko simple-price endpoint using encoded query parameters.
    let mut url = reqwest::Url::parse("https://api.coingecko.com/api/v3/simple/price")
        .expect("hardcoded CoinGecko URL must be valid");
    url.query_pairs_mut()
        .append_pair("ids", ids)
        .append_pair("vs_currencies", &quote_currencies);

    // Fetch, parse, cache, and return the external price response.
    match reqwest::Client::new().get(url).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<serde_json::Value>().await {
                Ok(value) => {
                    PRICE_CACHE.lock().unwrap().insert(
                        cache_key,
                        CacheEntry {
                            at: Instant::now(),
                            json: value.clone(),
                        },
                    );
                    Json(json!({"ok": true, "data": value}))
                }
                Err(e) => Json(json!({"ok": false, "error": format!("parse: {e}")})),
            }
        }
        Ok(response) => Json(json!({
            "ok": false,
            "error": format!("coingecko http {}", response.status())
        })),
        Err(e) => Json(json!({"ok": false, "error": format!("fetch: {e}")})),
    }
}

/// JSON request body accepted by `POST /api/pay/x402/confirm`.
#[derive(Deserialize)]
pub struct ConfirmReq {
    pub invoice_uid: String,
    pub tx_hash: String,
}

/// Defines the smart contract `Paid` event ABI used to decode transaction logs.
///
/// Solidity event:
/// `Paid(bytes32 invoiceUid, address payer, address creator, address admin,
/// address token, uint256 amountWei, string videoId)`
fn paid_event_abi() -> Event {
    Event {
        name: "Paid".to_string(),
        inputs: vec![
            EventParam {
                name: "invoiceUid".into(),
                kind: ParamType::FixedBytes(32),
                indexed: true,
            },
            EventParam {
                name: "payer".into(),
                kind: ParamType::Address,
                indexed: true,
            },
            EventParam {
                name: "creator".into(),
                kind: ParamType::Address,
                indexed: true,
            },
            EventParam {
                name: "admin".into(),
                kind: ParamType::Address,
                indexed: false,
            },
            EventParam {
                name: "token".into(),
                kind: ParamType::Address,
                indexed: false,
            },
            EventParam {
                name: "amountWei".into(),
                kind: ParamType::Uint(256),
                indexed: false,
            },
            EventParam {
                name: "videoId".into(),
                kind: ParamType::String,
                indexed: false,
            },
        ],
        anonymous: false,
    }
}

/// Confirms an x402 payment by verifying the on-chain transaction receipt.
///
/// Route: `POST /api/pay/x402/confirm`
///
/// Verification flow:
/// 1. Load the expected invoice from PostgreSQL.
/// 2. Fetch the transaction receipt from the configured EVM RPC endpoint.
/// 3. Require a successful transaction status.
/// 4. Locate a Paid event emitted by the configured x402 contract.
/// 5. Match the indexed invoice UID and decode amount and video ID.
/// 6. Compare the paid amount against the required amount.
/// 7. Update invoice status.
/// 8. Create purchase and allowlist records when fully paid.
pub async fn x402_confirm(
    State(st): State<VideoState>,
    Json(body): Json<ConfirmReq>,
) -> impl IntoResponse {
    // Reject incomplete confirmation requests.
    if body.invoice_uid.is_empty() || body.tx_hash.is_empty() {
        return Json(json!({"ok": false, "error": "invalid params"}));
    }

    // Load the invoice and required token amount used for receipt validation.
    let invoice = sqlx::query!(
        r#"
        SELECT id, user_id, video_id, invoice_uid, invoice_uid_hash, required_amount_wei
             , status, tx_hash
        FROM x402_invoices
        WHERE invoice_uid=$1
        LIMIT 1
        "#,
        body.invoice_uid
    )
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);

    let Some(invoice) = invoice else {
        return Json(json!({"ok": false, "error": "invoice not found"}));
    };

    if invoice.status == "paid" {
        let same_tx = invoice
            .tx_hash
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case(&body.tx_hash))
            .unwrap_or(false);
        return Json(json!({
            "ok": true,
            "status": "paid",
            "replayed": true,
            "same_tx": same_tx
        }));
    }

    // The HTTP RPC endpoint is required to fetch the transaction receipt.
    let rpc_url = std::env::var("X402_RPC_HTTP").unwrap_or_default();
    if rpc_url.is_empty() {
        return Json(json!({"ok": false, "error": "X402_RPC_HTTP not set"}));
    }

    // Build an Ethereum JSON-RPC request for `eth_getTransactionReceipt`.
    let rpc_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getTransactionReceipt",
        "params": [body.tx_hash]
    });

    let rpc_response = match reqwest::Client::new()
        .post(&rpc_url)
        .json(&rpc_payload)
        .send()
        .await
    {
        Ok(response) => response,
        Err(e) => return Json(json!({"ok": false, "error": format!("rpc: {e}")})),
    };

    let receipt: serde_json::Value = match rpc_response.json().await {
        Ok(receipt) => receipt,
        Err(e) => {
            return Json(json!({
                "ok": false,
                "error": format!("rpc parse: {e}")
            }))
        }
    };

    // Ethereum receipt status `0x1` means the transaction executed successfully.
    let transaction_status = receipt
        .pointer("/result/status")
        .and_then(|value| value.as_str())
        .unwrap_or("0x0");

    if transaction_status != "0x1" {
        return Json(json!({"ok": false, "error": "tx failed"}));
    }

    // Compute the expected event signature topic from the Paid ABI.
    let paid_event = paid_event_abi();
    let paid_signature =
        format!("0x{}", hex::encode(paid_event.signature().to_fixed_bytes())).to_lowercase();

    // Only events emitted by the configured x402 contract are accepted.
    let x402_contract = std::env::var("X402_CONTRACT_ADDRESS")
        .unwrap_or_default()
        .to_lowercase();

    let logs = receipt
        .pointer("/result/logs")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    // `topic[1]` must equal the bytes32 hash created from the invoice UUID.
    let expected_invoice_topic = invoice
        .invoice_uid_hash
        .as_deref()
        .unwrap_or("")
        .to_lowercase();

    let mut matched_amount_wei: Option<BigDecimal> = None;
    let mut matched_video_id: Option<String> = None;

    // Scan receipt logs until a matching Paid event is decoded.
    'scan: for log in logs {
        let emitting_address = log
            .get("address")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_lowercase();

        if emitting_address != x402_contract {
            continue;
        }

        let topics = log
            .get("topics")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        if topics.is_empty() {
            continue;
        }

        // `topic[0]` identifies the event type and must match Paid.
        let event_signature_topic = topics[0].as_str().unwrap_or("").to_lowercase();

        if event_signature_topic != paid_signature {
            continue;
        }

        // `topic[1]` contains the indexed invoice UID hash.
        if topics.len() < 2 {
            continue;
        }

        let invoice_topic = topics[1].as_str().unwrap_or("").to_lowercase();

        if invoice_topic != expected_invoice_topic {
            continue;
        }

        // Decode the non-indexed event data and convert topics into H256 values
        // required by ethabi RawLog.
        let data_hex = log
            .get("data")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let data_bytes = hex::decode(data_hex.trim_start_matches("0x")).unwrap_or_default();

        let topics_h256: Vec<H256> = topics
            .iter()
            .filter_map(|topic| topic.as_str())
            .filter_map(|topic| H256::from_str(topic).ok())
            .collect();

        let raw_log = RawLog {
            topics: topics_h256,
            data: data_bytes,
        };

        if let Ok(parsed_log) = paid_event.parse_log(raw_log) {
            // Find values by parameter name rather than array index so the code
            // remains readable and less sensitive to event field ordering.
            if let Some(parameter) = parsed_log
                .params
                .iter()
                .find(|parameter| parameter.name == "amountWei")
            {
                if let EventToken::Uint(amount) = &parameter.value {
                    matched_amount_wei = Some(
                        BigDecimal::from_str(&amount.to_string())
                            .unwrap_or_else(|_| BigDecimal::from(0)),
                    );
                }
            }

            if let Some(parameter) = parsed_log
                .params
                .iter()
                .find(|parameter| parameter.name == "videoId")
            {
                if let EventToken::String(video_id) = &parameter.value {
                    matched_video_id = Some(video_id.clone());
                }
            }

            break 'scan;
        }
    }

    let Some(paid_amount) = matched_amount_wei else {
        return Json(json!({"ok": false, "error": "Paid event not found"}));
    };

    // When the event contains a video ID, require it to match the invoice.
    if let Some(event_video_id) = matched_video_id.as_ref() {
        if event_video_id != &invoice.video_id {
            return Json(json!({
                "ok": false,
                "error": "mismatched videoId in event"
            }));
        }
    }

    // Compare the verified on-chain payment with the invoice requirement.
    let required_amount = invoice
        .required_amount_wei
        .unwrap_or_else(|| BigDecimal::from(0));
    let is_fully_paid = paid_amount >= required_amount;

    // Persist the transaction hash, accumulated paid amount, and final invoice
    // status. Accumulation allows a future top-up confirmation flow.
    let _ = sqlx::query!(
        r#"
        UPDATE x402_invoices
           SET status = $1,
               paid_at = NOW(),
               tx_hash = $2,
               paid_amount_wei = COALESCE(paid_amount_wei, 0) + $3
         WHERE id = $4
        "#,
        if is_fully_paid { "paid" } else { "underpaid" },
        body.tx_hash,
        &paid_amount,
        invoice.id
    )
    .execute(&st.pool)
    .await;

    // Return the missing amount when payment is below the invoice requirement.
    if !is_fully_paid {
        let missing_amount = (&required_amount - paid_amount).max(BigDecimal::from(0));
        return Json(json!({
            "ok": false,
            "underpaid": true,
            "missing_wei": missing_amount.to_string(),
            "message": "Payment below required amount. Please top up the remainder to unlock."
        }));
    }

    // Record the purchase idempotently so repeated confirmations do not create
    // duplicate purchase rows.
    let _ = sqlx::query!(
        r#"
        INSERT INTO purchases (user_id, video_id, created_at)
        VALUES ($1,$2,NOW())
        ON CONFLICT DO NOTHING
        "#,
        invoice.user_id,
        invoice.video_id
    )
    .execute(&st.pool)
    .await;

    // Resolve the buyer username because playback authorization currently uses
    // the `(video_id, username)` allowlist rather than a direct user ID relation.
    let username =
        sqlx::query_scalar::<_, Option<String>>(r#"SELECT username FROM users WHERE id=$1"#)
            .bind(&invoice.user_id)
            .fetch_one(&st.pool)
            .await
            .unwrap_or(None)
            .unwrap_or_default();

    // Insert the allowlist entry idempotently. `user_has_view_access` later uses
    // this record to authorize the buyer's playback request.
    if !username.is_empty() {
        let _ = sqlx::query!(
            r#"
            INSERT INTO allowlist (video_id, username)
            VALUES ($1,$2)
            ON CONFLICT (video_id, username) DO NOTHING
            "#,
            invoice.video_id,
            username
        )
        .execute(&st.pool)
        .await;
    }

    // Best-effort affiliate commission after x402 payment
    let affiliate_ref: Option<String> =
        sqlx::query("SELECT affiliate_ref FROM x402_invoices WHERE invoice_uid = $1 LIMIT 1")
            .bind(&body.invoice_uid)
            .fetch_optional(&st.pool)
            .await
            .ok()
            .flatten()
            .and_then(|r: sqlx::postgres::PgRow| {
                r.try_get::<Option<String>, _>("affiliate_ref")
                    .ok()
                    .flatten()
            });

    if let Some(ref_username) = affiliate_ref.as_deref().filter(|s| !s.is_empty()) {
        // We need creator_id + price_cents — re-read from invoice
        let inv_extra = sqlx::query(
            "SELECT creator_id, price_cents FROM x402_invoices WHERE invoice_uid = $1 LIMIT 1",
        )
        .bind(&body.invoice_uid)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten();

        if let Some(row) = inv_extra {
            let creator_id: String = row.try_get("creator_id").unwrap_or_default();
            let price_cents: i64 = row.try_get("price_cents").unwrap_or(0);

            if let Err(e) = commission::process_affiliate_commission(
                &st.pool,
                &invoice.video_id,
                &invoice.user_id,
                &creator_id,
                price_cents,
                ref_username,
                "x402",
                Some(&body.invoice_uid),
            )
            .await
            {
                tracing::warn!("x402 affiliate commission skipped: {e}");
            }
        }
    }

    Json(json!({"ok": true, "status": "paid"}))
}

// ─── GET /api/pay/all_options?video_id= ──────────────────────────────────────
// Returns all available payment methods for one video in a single call:
//   - wallet: buyer's current balance, whether they can afford it
//   - x402:   available if tokens are configured
//   - fiat:   available providers (from PAYMENT_PLUGINS env)
//
// Used by watch.html to decide which payment tabs to render.

pub async fn all_options(
    State(st): State<VideoState>,
    cookies: Cookies,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let payment_settings = load_payment_settings(&st.pool).await;
    let video_id = q.get("video_id").cloned().unwrap_or_default();
    if video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    // Load video price and owner
    let video_row =
        sqlx::query("SELECT v.price_cents, v.owner_id FROM videos v WHERE v.id = $1 LIMIT 1")
            .bind(&video_id)
            .fetch_optional(&st.pool)
            .await;

    let video_row = match video_row {
        Ok(Some(r)) => r,
        Ok(None) => return Json(json!({"ok": false, "error": "video not found"})),
        Err(e) => return Json(json!({"ok": false, "error": format!("db: {e}")})),
    };

    let price_cents: i64 = video_row.try_get("price_cents").unwrap_or(0);
    let owner_id: String = video_row.try_get("owner_id").unwrap_or_default();

    // Wallet balance (null if not logged in)
    let current_user = sessions::current_user_id(&st.pool, &st.cfg, &cookies).await;
    let (wallet_balance, is_owner, already_purchased) = if let Some((uid, _)) = &current_user {
        let bal: i64 = sqlx::query_scalar("SELECT balance_cents FROM users WHERE id = $1")
            .bind(uid)
            .fetch_optional(&st.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(0);

        let purchased: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM purchases WHERE user_id = $1 AND video_id = $2",
        )
        .bind(uid)
        .bind(&video_id)
        .fetch_one(&st.pool)
        .await
        .unwrap_or(0);

        (Some(bal), uid == &owner_id, purchased > 0)
    } else {
        (None, false, false)
    };

    // X402 tokens
    let tokens = sqlx::query(
        r#"SELECT chain, chain_id, symbol, decimals,
                  COALESCE(erc20_address, erc20) AS erc20_address
           FROM pay_tokens WHERE is_active = TRUE ORDER BY chain_id, symbol"#,
    )
    .fetch_all(&st.pool)
    .await
    .unwrap_or_default();

    let token_list: Vec<serde_json::Value> = tokens
        .iter()
        .map(|t| {
            json!({
                "chain":     t.try_get::<String, _>("chain").unwrap_or_default(),
                "chain_id":  t.try_get::<i64,    _>("chain_id").unwrap_or(0),
                "symbol":    t.try_get::<String, _>("symbol").unwrap_or_default(),
                "decimals":  t.try_get::<i32,    _>("decimals").unwrap_or(18),
                "erc20":     t.try_get::<Option<String>, _>("erc20_address").unwrap_or(None),
            })
        })
        .collect();

    // Fiat providers from env (admin-configured)
    let mut fiat_providers: Vec<String> = Vec::new();
    if payment_settings.paypal_enabled {
        fiat_providers.push("paypal".to_string());
    }
    if payment_settings.stripe_enabled {
        fiat_providers.push("stripe".to_string());
    }
    if payment_settings.midtrans_enabled {
        fiat_providers.push("midtrans".to_string());
    }
    if payment_settings.xendit_enabled {
        fiat_providers.push("xendit".to_string());
    }

    let cents_display =
        |c: i64| -> String { format!("${}.{:02}", c / 100, (c % 100).unsigned_abs()) };

    Json(json!({
        "ok": true,
        "video_id":        video_id,
        "price_cents":     price_cents,
        "price_display":   cents_display(price_cents),
        "is_owner":        is_owner,
        "already_purchased": already_purchased,
        "wallet": {
            "available":       payment_settings.wallet_payment_enabled,
            "balance_cents":   wallet_balance,
            "balance_display": wallet_balance.map(|b| cents_display(b)),
            "can_afford":      wallet_balance.map(|b| payment_settings.wallet_payment_enabled && b >= price_cents && !is_owner),
        },
        "x402": {
            "available": payment_settings.x402_enabled && !token_list.is_empty(),
            "tokens":    token_list,
        },
        "fiat": {
            "available": !fiat_providers.is_empty(),
            "providers": fiat_providers,
        }
    }))
}
