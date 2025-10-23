// src/services/x402_watcher.rs
// src/services/x402_watcher.rs
use anyhow::Result;
use ethers::prelude::*;
use ethers::providers::{Provider, Ws};
use sqlx::{PgPool, Row}; // Row diperlukan untuk row.get::<T,_>()
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tracing::{error, info};
use std::time::Duration;
use hex;

// Pakai JSON ABI agar parser stabil
abigen!(
    X402Splitter,
    r#"[{
      "anonymous": false,
      "inputs": [
        { "indexed": true,  "internalType": "bytes32", "name": "invoiceUid", "type": "bytes32" },
        { "indexed": true,  "internalType": "address", "name": "payer",      "type": "address" },
        { "indexed": true,  "internalType": "address", "name": "creator",    "type": "address" },
        { "indexed": false, "internalType": "address", "name": "admin",      "type": "address" },
        { "indexed": false, "internalType": "address", "name": "token",      "type": "address" },
        { "indexed": false, "internalType": "uint256", "name": "amountWei",  "type": "uint256" },
        { "indexed": false, "internalType": "string",  "name": "videoId",    "type": "string" }
      ],
      "name": "Paid",
      "type": "event"
    }]"#
);

/// Jalankan watcher X402 dengan auto-reconnect.
pub async fn run_watcher(pool: PgPool, wss_url: String, contract_addr: Address) -> Result<()> {
    info!("🚀 X402 watcher initialized, WSS={}", wss_url);

    loop {
        match watch_once(&pool, &wss_url, contract_addr).await {
            Ok(_) => info!("✅ Watcher stopped gracefully, restarting in 10s..."),
            Err(e) => error!("💥 Watcher error: {e}, reconnecting in 10s..."),
        }
        sleep(Duration::from_secs(10)).await;
    }
}

async fn watch_once(pool: &PgPool, wss_url: &str, contract_addr: Address) -> Result<()> {
    let ws = Ws::connect(wss_url).await?;
    let provider = Provider::new(ws);
    let client = std::sync::Arc::new(provider);
    let contract = X402Splitter::new(contract_addr, client);

    let mut stream = contract.event::<PaidFilter>().stream().await?;
    info!("🎧 Listening for Paid(...) events on {}", contract_addr);

    while let Some(event) = stream.next().await {
        match event {
            Ok(ev) => {
                // Normalisasi hash: 0x + lowercase
                let bytes: [u8; 32] = ev.invoice_uid.into(); // H256 -> [u8;32]
                let invoice_hash = format!("0x{}", hex::encode(bytes));
                let payer = format!("{:?}", ev.payer); // atau ev.payer.encode_hex::<String>()
                let video_id = ev.video_id.clone();

                info!("💰 Paid: hash={}, payer={}, video_id={}", invoice_hash, payer, video_id);

                if let Err(e) = handle_paid_event(pool, &invoice_hash, &payer, &video_id).await {
                    error!("⚠️ handle_paid_event error: {}", e);
                }
            }
            Err(e) => {
                error!("❌ Event stream error: {}", e);
                break; // trigger reconnect
            }
        }
    }

    Ok(())
}

async fn handle_paid_event(pool: &PgPool, invoice_hash: &str, payer: &str, video_id: &str) -> Result<()> {
    // gunakan sqlx::query (runtime-checked) agar build tidak perlu akses DB
    let rec = sqlx::query(
        r#"SELECT id, user_id 
           FROM x402_invoices 
           WHERE LOWER(invoice_uid_hash) = $1 
           LIMIT 1"#,
    )
    .bind(invoice_hash)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = rec {
        let inv_id: i64 = row.get("id");
        let user_id: String = row.get("user_id");

        // Update invoice -> paid
        sqlx::query(
            r#"UPDATE x402_invoices 
               SET status='paid', paid_at=NOW(), payer_address=$1 
               WHERE id=$2"#,
        )
        .bind(payer)
        .bind(inv_id)
        .execute(pool)
        .await?;

        // Ambil username user
        let uname = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT username FROM users WHERE id=$1"#,
        )
        .bind(&user_id)
        .fetch_one(pool)
        .await?
        .unwrap_or_default();

        if !uname.is_empty() {
            // purchases (idempotent)
            sqlx::query(
                r#"INSERT INTO purchases (user_id, video_id, created_at)
                   VALUES ($1,$2,NOW())
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(&user_id)
            .bind(video_id)
            .execute(pool)
            .await?;

            // allowlist (idempotent)
            sqlx::query(
                r#"INSERT INTO allowlist (video_id, username)
                   VALUES ($1,$2)
                   ON CONFLICT (video_id, username) DO NOTHING"#,
            )
            .bind(video_id)
            .bind(&uname)
            .execute(pool)
            .await?;

            info!("✅ Access granted for {} on video {}", uname, video_id);
        } else {
            info!("ℹ️ Invoice matched but username not found (user_id={})", user_id);
        }
    } else {
        error!("⚠️ No matching invoice for hash {}", invoice_hash);
    }

    Ok(())
}
