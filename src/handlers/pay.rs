// src/handlers/pay.rs
// src/handlers/pay.rs

use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::sessions;

// ===== tambahan util & cache untuk harga =====
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ===== HEX utils =====
use hex;

// ===== tipe untuk NUMERIC =====
use sqlx::types::BigDecimal;
use std::str::FromStr;

// ===== eth log decoding (ethabi) =====
use ethabi::{Event, EventParam, ParamType, RawLog, Token as EvtToken};
use ethereum_types::H256;

// Gunakan VideoState dari modul video.
use crate::handlers::video::VideoState;

/* ============================================================
   GET /api/pay/options?video_id=VID
   ============================================================ */
pub async fn pay_options(
    State(st): State<VideoState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let video_id = q.get("video_id").cloned().unwrap_or_default();
    if video_id.is_empty() {
        return Json(json!({"ok": false, "error": "video_id required"}));
    }

    // Ambil meta video + owner
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
        Ok(r) => r,
        Err(e) => {
            return Json(json!({"ok": false, "error": format!("db error: {e}")}));
        }
    };

    let Some(vmeta) = row else {
        return Json(json!({"ok": false, "error": "video not found"}));
    };

    // Ambil daftar token (kompatibel erc20_address / erc20)
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

    Json(json!({
        "ok": true,
        "video_id": video_id,
        "price_cents": vmeta.try_get::<i64,_>("price_cents").unwrap_or(0),
        "creator_id": vmeta.try_get::<String,_>("owner_id").unwrap_or_default(),
        "creator_wallet": vmeta.try_get::<Option<String>,_>("wallet_account").ok().flatten(),
        "creator_chain_id": vmeta.try_get::<Option<i64>,_>("wallet_chain_id").ok().flatten(),
        "tokens": tokens.iter().map(|t| json!({
            "chain": t.try_get::<String,_>("chain").unwrap_or_default(),
            "chain_id": t.try_get::<i64,_>("chain_id").unwrap_or_default(),
            "symbol": t.try_get::<String,_>("symbol").unwrap_or_default(),
            "decimals": t.try_get::<i32,_>("decimals").unwrap_or_default(),
            // Frontend field "erc20" diisi dari kolom yg kompatibel
            "erc20": t.try_get::<Option<String>,_>("erc20_address").ok().flatten(),
        })).collect::<Vec<_>>()
    }))
}

/* ============================================================
   POST /api/pay/x402/start
   ============================================================ */
#[derive(Deserialize)]
pub struct StartPayReq {
    pub video_id: String,
    pub chain_id: i64,
    pub symbol: String,
    pub token_address: Option<String>,
    pub payer_address: String, // <— diperlukan untuk payload signature
}

#[derive(Serialize)]
pub struct StartPayResp {
    pub ok: bool,
    pub invoice_uid: String,
    pub invoice_uid_bytes32: String, // 0x…32 bytes keccak(invoice_uid)
    pub video_id: String,
    pub chain_id: i64,
    pub symbol: String,
    pub token_address: Option<String>,
    pub amount_wei: String,          // nominal yang harus dibayar (string decimal)
    pub min_amount_wei: String,      // sama dengan amount_wei, untuk verifikasi onchain
    pub deadline: u64,               // unix ts
    pub v: u8,
    pub r: String,
    pub s: String,
    pub split_creator_bp: i32,
    pub split_admin_bp: i32,
    pub x402_contract: String,
    pub creator_wallet: String,
}

// ===== ethers untuk keccak256, abi-encode & signing =====
use ethers::abi::{encode as abi_encode, Token as AbiToken};
use ethers::core::utils::keccak256;
// use ethers::signers::{LocalWallet, Signer};
use ethers::types::{Address, Signature, U256};
use ethers::signers::LocalWallet;

pub async fn x402_start(
    State(st): State<VideoState>,
    cookies: Cookies,
    Json(body): Json<StartPayReq>,
) -> impl IntoResponse {
    let (buyer_id, _) = match sessions::current_user_id(&st.pool, &st.cfg, &cookies).await { 
        Some(v) => v,
        None => return Json(json!({"ok": false, "error": "not logged in"})),
    };

    if body.video_id.is_empty() || body.chain_id == 0 || body.symbol.is_empty() {
        return Json(json!({"ok": false, "error": "invalid params"}));
    }

    // ===== validasi payer_address dari frontend =====
    let payer_addr = match Address::from_str(&body.payer_address) {
        Ok(a) => a,
        Err(_) => return Json(json!({"ok": false, "error": "invalid payer_address"})),
    };

    // ===== ambil meta video & kreator =====
    let vmeta = sqlx::query(
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

    let Some(vmeta) = vmeta else {
        return Json(json!({"ok": false, "error": "video not found"}));
    };

    let creator_wallet_str = match vmeta.try_get::<Option<String>, _>("wallet_account") {
        Ok(Some(w)) if !w.is_empty() => w,
        _ => return Json(json!({"ok": false, "error": "creator has no wallet"})),
    };
    let creator_addr = match Address::from_str(&creator_wallet_str) {
        Ok(a) => a,
        Err(_) => return Json(json!({"ok": false, "error": "invalid creator wallet"})),
    };

    // ===== token info =====
    let tinfo = sqlx::query(
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

    let Some(tinfo) = tinfo else {
        return Json(json!({"ok": false, "error": "token not supported"}));
    };

    let decimals = tinfo.try_get::<i32, _>("decimals").unwrap_or(18) as u32;
    let price_cents: i64 = vmeta.try_get::<i64, _>("price_cents").unwrap_or(0);

    // cents -> wei (ceil)
    let token_amount_wei: u128 = if price_cents <= 0 {
        0
    } else {
        let num = (price_cents as u128).saturating_mul(10u128.pow(decimals));
        num.saturating_add(99) / 100
    };
    if token_amount_wei == 0 {
        return Json(json!({"ok": false, "error": "calculated amount is zero"}));
    }

    // ===== buat invoice uid + hash =====
    let invoice_uid = Uuid::new_v4().to_string();
    let invoice_uid_bytes32 = H256::from_slice(&keccak256(invoice_uid.as_bytes()));

    // simpan ke DB
    let token_amount_bd = BigDecimal::from_str(&token_amount_wei.to_string())
        .unwrap_or_else(|_| BigDecimal::from(0));
    let invoice_uid_hash_hex = format!("{:#066x}", invoice_uid_bytes32);

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
    .bind(vmeta.try_get::<String, _>("owner_id").unwrap_or_default())
    .bind(body.chain_id)
    .bind(&body.symbol)
    .bind(&body.token_address)
    .bind(price_cents)
    .bind(&token_amount_bd)
    .bind(&token_amount_bd)
    .execute(&st.pool)
    .await;

    // ===== siapkan nilai yang ditandatangani =====
    let x402_contract = std::env::var("X402_CONTRACT_ADDRESS").unwrap_or_default();
    let _contract_addr = match Address::from_str(&x402_contract) {
        Ok(a) => a,
        Err(_) => Address::zero(), // front-end juga mengecek code!=0
    };

    let token_addr = if let Some(t) = &body.token_address {
        Address::from_str(t).unwrap_or(Address::zero())
    } else {
        Address::zero() // native
    };

    let creator_bp: u16 = 9000;
    let min_amount_wei = U256::from_dec_str(&token_amount_wei.to_string()).unwrap();
    let deadline: u64 = (chrono::Utc::now().timestamp() as u64) + 900; // 15 menit
    let video_hash = H256::from_slice(&keccak256(body.video_id.as_bytes()));

    // ABI-encode sesuai _verify(...) di kontrak
    // (invoiceUid, token, minAmountWei, creator, creatorBp, keccak256(videoId), payer, deadline)
    let encoded = abi_encode(&[
        AbiToken::FixedBytes(invoice_uid_bytes32.as_bytes().to_vec()),
        AbiToken::Address(token_addr),
        AbiToken::Uint(min_amount_wei),
        AbiToken::Address(creator_addr),
        AbiToken::Uint(U256::from(creator_bp)),
        AbiToken::FixedBytes(video_hash.as_bytes().to_vec()),
        AbiToken::Address(payer_addr),
        AbiToken::Uint(U256::from(deadline)),
    ]);
    let msg_hash = H256::from_slice(&keccak256(&encoded));
    // EIP-191 "\x19Ethereum Signed Message:\n32"
    let eth_hash = H256::from_slice(
        &keccak256([b"\x19Ethereum Signed Message:\n32", msg_hash.as_bytes()].concat()),
    );

    // Sign pakai private key admin
    let admin_pk = std::env::var("X402_ADMIN_PRIVKEY").unwrap_or_default();
    if admin_pk.is_empty() {
        return Json(json!({"ok": false, "error": "X402_ADMIN_PRIVKEY not set"}));
    }
    let admin_wallet: LocalWallet = match admin_pk.parse() {
        Ok(w) => w,
        Err(_) => return Json(json!({"ok": false, "error": "bad admin privkey"})),
    };
    // sign_hash adalah sinkron (tidak perlu .await)
    let sig: Signature = match admin_wallet.sign_hash(eth_hash) {
        Ok(s) => s,
        Err(e) => return Json(json!({"ok": false, "error": format!("sign: {e}")})),
    };

    let resp = StartPayResp {
        ok: true,
        invoice_uid,
        invoice_uid_bytes32: format!("{:#066x}", invoice_uid_bytes32),
        video_id: body.video_id,
        chain_id: body.chain_id,
        symbol: body.symbol,
        token_address: body.token_address,
        amount_wei: token_amount_wei.to_string(),
        min_amount_wei: min_amount_wei.to_string(),
        deadline,
        v: sig.v as u8,
        r: format!("{:#066x}", sig.r),
        s: format!("{:#066x}", sig.s),
        split_creator_bp: 9000,
        split_admin_bp: 1000,
        x402_contract,
        creator_wallet: creator_wallet_str,
    };

    Json(json!(resp))
}

/* ============================================================
   GET /api/crypto_price?ids=ethereum,usd-coin&vs=idr,usd
   ============================================================ */

#[derive(Clone, Debug)]
struct CacheEntry {
    at: Instant,
    json: serde_json::Value,
}

static PRICE_CACHE: Lazy<Mutex<HashMap<String, CacheEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Deserialize)]
pub struct PriceQs {
    pub ids: String,        // contoh: ethereum,usd-coin,polygon-pos
    pub vs: Option<String>, // contoh: idr,usd (default: idr)
}

pub async fn crypto_price(Query(q): Query<PriceQs>) -> impl IntoResponse {
    let ids = q.ids.trim();
    if ids.is_empty() {
        return Json(json!({"ok": false, "error": "ids required"}));
    }
    let vs = q.vs.unwrap_or_else(|| "idr".to_string());

    let key = format!("{}|{}", ids, vs);

    // 1) Cache 60 detik
    if let Some(hit) = PRICE_CACHE.lock().unwrap().get(&key) {
        if hit.at.elapsed() < Duration::from_secs(60) {
            return Json(json!({"ok": true, "data": hit.json}));
        }
    }

    // 2) Panggil CoinGecko
    let mut url = reqwest::Url::parse("https://api.coingecko.com/api/v3/simple/price")
        .expect("hardcoded url valid");
    url.query_pairs_mut()
        .append_pair("ids", ids)
        .append_pair("vs_currencies", &vs);

    match reqwest::Client::new().get(url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>().await {
            Ok(val) => {
                PRICE_CACHE.lock().unwrap().insert(
                    key.clone(),
                    CacheEntry {
                        at: Instant::now(),
                        json: val.clone(),
                    },
                );
                Json(json!({"ok": true, "data": val}))
            }
            Err(e) => Json(json!({"ok": false, "error": format!("parse: {e}")})),
        },
        Ok(resp) => {
            let code = resp.status();
            Json(json!({"ok": false, "error": format!("coingecko http {code}")}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("fetch: {e}")})),
    }
}

/* ============================================================
   POST /api/pay/x402/confirm
   ============================================================ */
#[derive(Deserialize)]
pub struct ConfirmReq {
    pub invoice_uid: String,
    pub tx_hash: String,
}

fn paid_event_abi() -> Event {
    // event Paid(bytes32 indexed invoiceUid, address indexed payer, address indexed creator,
    //            address admin, address token, uint256 amountWei, string videoId)
    Event {
        name: "Paid".to_string(),
        inputs: vec![
            EventParam { name: "invoiceUid".into(), kind: ParamType::FixedBytes(32), indexed: true },
            EventParam { name: "payer".into(),      kind: ParamType::Address,        indexed: true },
            EventParam { name: "creator".into(),    kind: ParamType::Address,        indexed: true },
            EventParam { name: "admin".into(),      kind: ParamType::Address,        indexed: false },
            EventParam { name: "token".into(),      kind: ParamType::Address,        indexed: false },
            EventParam { name: "amountWei".into(),  kind: ParamType::Uint(256),      indexed: false },
            EventParam { name: "videoId".into(),    kind: ParamType::String,         indexed: false },
        ],
        anonymous: false,
    }
}

pub async fn x402_confirm(
    State(st): State<VideoState>,
    Json(b): Json<ConfirmReq>,
) -> impl IntoResponse {
    if b.invoice_uid.is_empty() || b.tx_hash.is_empty() {
        return Json(json!({"ok": false, "error": "invalid params"}));
    }

    // Ambil invoice (butuh required_amount_wei untuk validasi)
    let inv = sqlx::query!(
        r#"
        SELECT id, user_id, video_id, invoice_uid, invoice_uid_hash, required_amount_wei
        FROM x402_invoices
        WHERE invoice_uid=$1
        LIMIT 1
        "#,
        b.invoice_uid
    )
    .fetch_optional(&st.pool)
    .await
    .unwrap_or(None);

    let Some(inv) = inv else {
        return Json(json!({"ok": false, "error": "invoice not found"}));
    };

    // RPC HTTP wajib tersedia
    let rpc = std::env::var("X402_RPC_HTTP").unwrap_or_default();
    if rpc.is_empty() {
        return Json(json!({"ok": false, "error": "X402_RPC_HTTP not set"}));
    }

    // Ambil receipt
    let payload = serde_json::json!({
      "jsonrpc":"2.0","id":1,"method":"eth_getTransactionReceipt","params":[b.tx_hash]
    });

    let resp = match reqwest::Client::new().post(&rpc).json(&payload).send().await {
        Ok(r) => r,
        Err(e) => return Json(json!({"ok": false, "error": format!("rpc: {e}")})),
    };

    let receipt: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return Json(json!({"ok": false, "error": format!("rpc parse: {e}")})),
    };

    let status_hex = receipt.pointer("/result/status").and_then(|v| v.as_str()).unwrap_or("0x0");
    if status_hex != "0x1" {
        return Json(json!({"ok": false, "error": "tx failed"}));
    }

    // Decode log Paid & validasi invoiceUid + ambil amountWei & videoId
    let paid_ev = paid_event_abi();
    let paid_sig_lc = format!("0x{}", hex::encode(paid_ev.signature().to_fixed_bytes())).to_lowercase();

    let x402_contract = std::env::var("X402_CONTRACT_ADDRESS")
        .unwrap_or_default()
        .to_lowercase();

    let logs = receipt
        .pointer("/result/logs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Expected topic[1] (invoiceUid) adalah keccak(invoice_uid string) — sama dg invoice_uid_hash
    // invoice_uid_hash disimpan sebagai "0x..." -> normalisasi ke lowercase untuk perbandingan.
    let expected_invoice_topic = inv
        .invoice_uid_hash
        .as_deref()
        .unwrap_or("")
        .to_lowercase();

    let mut matched_amount_wei: Option<BigDecimal> = None;
    let mut matched_video_id: Option<String> = None;

    'scan: for lg in logs {
        let addr = lg.get("address").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        if addr != x402_contract {
            continue;
        }

        let topics = lg.get("topics").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if topics.is_empty() {
            continue;
        }

        // topic[0] harus = signature event "Paid"
        let t0_lc = topics[0].as_str().unwrap_or("").to_lowercase();
        if t0_lc != paid_sig_lc {
            continue;
        }

        // Pastikan topics[1] = invoiceUid (bytes32) cocok
        if topics.len() >= 2 {
            let t1 = topics[1].as_str().unwrap_or("").to_lowercase();
            if t1 != expected_invoice_topic {
                continue;
            }
        } else {
            continue;
        }

        // Decode data
        let data_hex = lg.get("data").and_then(|v| v.as_str()).unwrap_or("");
        let data_bytes = hex::decode(data_hex.trim_start_matches("0x")).unwrap_or_default();

        // Konversi semua topics ke H256 untuk RawLog
        let topics_h256: Vec<H256> = topics
            .iter()
            .filter_map(|t| t.as_str())
            .filter_map(|s| H256::from_str(s).ok())
            .collect();

        let raw = RawLog {
            topics: topics_h256,
            data: data_bytes,
        };

        if let Ok(parsed) = paid_ev.parse_log(raw) {
            // Ambil amountWei & videoId by name (lebih aman daripada by index)
            if let Some(p) = parsed.params.iter().find(|p| p.name == "amountWei") {
                if let EvtToken::Uint(u) = &p.value {
                    let amt_str = u.to_string();
                    matched_amount_wei =
                        Some(BigDecimal::from_str(&amt_str).unwrap_or(BigDecimal::from(0)));
                }
            }
            if let Some(p) = parsed.params.iter().find(|p| p.name == "videoId") {
                if let EvtToken::String(s) = &p.value {
                    matched_video_id = Some(s.clone());
                }
            }
            break 'scan;
        }
    }

    let Some(paid_amt) = matched_amount_wei else {
        return Json(json!({"ok": false, "error": "Paid event not found"}));
    };

    // (Opsional): pastikan videoId pada event = invoice.video_id
    if let Some(ev_vid) = matched_video_id.as_ref() {
        if ev_vid != &inv.video_id {
            return Json(json!({"ok": false, "error": "mismatched videoId in event"}));
        }
    }

    // Bandingkan: paid >= required ?
    let required = inv.required_amount_wei.unwrap_or(BigDecimal::from(0));
    let is_enough = paid_amt >= required;

    // Update invoice status + catat tx & akumulasi paid_amount_wei
    let _ = sqlx::query!(
        r#"
        UPDATE x402_invoices
           SET status = $1,
               paid_at = NOW(),
               tx_hash = $2,
               paid_amount_wei = COALESCE(paid_amount_wei, 0) + $3
         WHERE id = $4
        "#,
        if is_enough { "paid" } else { "underpaid" },
        b.tx_hash,
        &paid_amt,
        inv.id
    )
    .execute(&st.pool)
    .await;

    if !is_enough {
        let missing = (&required - paid_amt).max(BigDecimal::from(0));
        return Json(json!({
            "ok": false,
            "underpaid": true,
            "missing_wei": missing.to_string(),
            "message": "Payment below required amount. Please top up the remainder to unlock."
        }));
    }

    // ===== Unlock (idempotent) =====
    let _ = sqlx::query!(
        r#"INSERT INTO purchases (user_id, video_id, created_at)
           VALUES ($1,$2,NOW())
           ON CONFLICT DO NOTHING"#,
        inv.user_id,
        inv.video_id
    )
    .execute(&st.pool)
    .await;

    // allowlist by username
    let uname = sqlx::query_scalar::<_, Option<String>>(r#"SELECT username FROM users WHERE id=$1"#)
        .bind(&inv.user_id)
        .fetch_one(&st.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_default();

    if !uname.is_empty() {
        let _ = sqlx::query!(
            r#"INSERT INTO allowlist (video_id, username)
               VALUES ($1,$2)
               ON CONFLICT (video_id, username) DO NOTHING"#,
            inv.video_id,
            uname
        )
        .execute(&st.pool)
        .await;
    }

    Json(json!({"ok": true, "status": "paid"}))
}
