use anyhow::{anyhow, bail, Result};
use ethereum_types::H256;
use ethers::abi::{encode as abi_encode, Token as AbiToken};
use ethers::core::utils::keccak256;
use ethers::signers::LocalWallet;
use ethers::types::{Address, Signature, U256};
use serde_json::json;
use sqlx::{types::BigDecimal, PgPool, Row};
use std::{env, str::FromStr};
use uuid::Uuid;

use crate::plugins::payment::{
    models::{
        ConfirmPaymentRequest, CreateInvoiceRequest, Invoice, PaymentPluginCapability,
        PaymentResult, PaymentStatus,
    },
    traits::PaymentPlugin,
};

#[derive(Clone, Debug)]
pub struct X402PaymentPlugin {
    pool: Option<PgPool>,
    configured: bool,
    missing_env: Vec<String>,
}

impl X402PaymentPlugin {
    pub fn from_env() -> Self {
        Self::from_env_with_pool(None)
    }

    pub fn from_env_with_pool(pool: Option<PgPool>) -> Self {
        let required = ["X402_CONTRACT_ADDRESS", "X402_RPC_HTTP", "X402_ADMIN_PRIVKEY"];
        let missing_env = required
            .iter()
            .filter(|key| env::var(key).unwrap_or_default().is_empty())
            .map(|key| key.to_string())
            .collect::<Vec<_>>();
        Self {
            pool,
            configured: missing_env.is_empty(),
            missing_env,
        }
    }

    fn required_metadata<'a>(request: &'a CreateInvoiceRequest, key: &str) -> Result<&'a str> {
        request
            .metadata
            .get(key)
            .map(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("missing x402 metadata field: {key}"))
    }
}

impl Default for X402PaymentPlugin {
    fn default() -> Self { Self::from_env() }
}

#[async_trait::async_trait]
impl PaymentPlugin for X402PaymentPlugin {
    fn provider_key(&self) -> &'static str { "x402" }
    fn display_name(&self) -> &'static str { "x402" }
    fn capability(&self) -> PaymentPluginCapability {
        PaymentPluginCapability {
            provider: self.provider_key().to_string(),
            display_name: self.display_name().to_string(),
            configured: self.configured && self.pool.is_some(),
            environment: env::var("X402_CHAIN_ID").unwrap_or_else(|_| "evm".to_string()),
            api_base_url: env::var("X402_RPC_HTTP").ok(),
            supports_redirect_checkout: false,
            supports_webhook_confirmation: false,
            supports_manual_confirmation: true,
            supported_currencies: vec!["USDC".into(), "MATIC".into(), "ETH".into()],
            required_env: vec!["X402_CONTRACT_ADDRESS".into(), "X402_RPC_HTTP".into(), "X402_ADMIN_PRIVKEY".into()],
            missing_env: self.missing_env.clone(),
        }
    }

    async fn create_invoice(&self, request: CreateInvoiceRequest) -> Result<Invoice> {
        if !self.configured {
            bail!("x402 plugin is not configured. Missing env: {:?}", self.missing_env);
        }

        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| anyhow!("x402 plugin has no database pool"))?;

        let chain_id = Self::required_metadata(&request, "chain_id")?.parse::<i64>()?;
        let symbol = Self::required_metadata(&request, "symbol")?.to_string();
        let payer_address_string = Self::required_metadata(&request, "payer_address")?.to_string();
        let token_address_string = request
            .metadata
            .get("token_address")
            .cloned()
            .filter(|value| !value.trim().is_empty());

        let payer_address = Address::from_str(&payer_address_string)
            .map_err(|_| anyhow!("invalid x402 payer_address"))?;

        let video_metadata = sqlx::query(
            r#"
            SELECT v.price_cents, v.owner_id, u.wallet_account
            FROM videos v
            JOIN users u ON u.id = v.owner_id
            WHERE v.id = $1
            LIMIT 1
            "#,
        )
        .bind(&request.video_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("video not found"))?;

        let creator_wallet_string = video_metadata
            .try_get::<Option<String>, _>("wallet_account")?
            .filter(|wallet| !wallet.trim().is_empty())
            .ok_or_else(|| anyhow!("creator has no wallet"))?;

        let creator_address = Address::from_str(&creator_wallet_string)
            .map_err(|_| anyhow!("invalid creator wallet"))?;

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
        .bind(chain_id)
        .bind(&symbol)
        .bind(&token_address_string)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("token not supported"))?;

        let decimals = token_info.try_get::<i32, _>("decimals").unwrap_or(18) as u32;
        if decimals > 38 {
            bail!("unsupported token decimals: {decimals}");
        }

        let fallback_price_cents = video_metadata.try_get::<i64, _>("price_cents").unwrap_or(0);
        let price_cents = if request.amount_cents > 0 {
            request.amount_cents
        } else {
            fallback_price_cents
        };

        let token_amount_wei: u128 = if price_cents <= 0 {
            0
        } else {
            let numerator = (price_cents as u128).saturating_mul(10u128.pow(decimals));
            numerator.saturating_add(99) / 100
        };

        if token_amount_wei == 0 {
            bail!("calculated x402 token amount is zero");
        }

        let invoice_uid = Uuid::new_v4().to_string();
        let invoice_uid_bytes32 = H256::from_slice(&keccak256(invoice_uid.as_bytes()));
        let invoice_uid_hash_hex = format!("{:#066x}", invoice_uid_bytes32);
        let token_amount_decimal = BigDecimal::from_str(&token_amount_wei.to_string())
            .unwrap_or_else(|_| BigDecimal::from(0));
        let creator_id = video_metadata.try_get::<String, _>("owner_id").unwrap_or_default();

        sqlx::query(
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
        .bind(&request.user_id)
        .bind(&request.video_id)
        .bind(&creator_id)
        .bind(chain_id)
        .bind(&symbol)
        .bind(&token_address_string)
        .bind(price_cents)
        .bind(&token_amount_decimal)
        .bind(&token_amount_decimal)
        .execute(pool)
        .await?;

        let x402_contract = env::var("X402_CONTRACT_ADDRESS").unwrap_or_default();
        let token_address = token_address_string
            .as_deref()
            .and_then(|value| Address::from_str(value).ok())
            .unwrap_or(Address::zero());
        let creator_basis_points: u16 = 9000;
        let minimum_amount_wei = U256::from_dec_str(&token_amount_wei.to_string())?;
        let deadline: u64 = (chrono::Utc::now().timestamp() as u64) + 900;
        let video_hash = H256::from_slice(&keccak256(request.video_id.as_bytes()));

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

        let message_hash = H256::from_slice(&keccak256(&encoded_payload));
        let ethereum_signed_hash = H256::from_slice(&keccak256(
            [b"\x19Ethereum Signed Message:\n32", message_hash.as_bytes()].concat(),
        ));

        let admin_private_key = env::var("X402_ADMIN_PRIVKEY").unwrap_or_default();
        let admin_wallet: LocalWallet = admin_private_key
            .parse()
            .map_err(|_| anyhow!("bad X402_ADMIN_PRIVKEY"))?;
        let signature: Signature = admin_wallet.sign_hash(ethereum_signed_hash)?;

        let raw = json!({
            "invoice_uid": invoice_uid,
            "invoice_uid_bytes32": format!("{:#066x}", invoice_uid_bytes32),
            "video_id": request.video_id,
            "chain_id": chain_id,
            "symbol": symbol,
            "token_address": token_address_string,
            "amount_wei": token_amount_wei.to_string(),
            "min_amount_wei": minimum_amount_wei.to_string(),
            "deadline": deadline,
            "v": signature.v as u8,
            "r": format!("{:#066x}", signature.r),
            "s": format!("{:#066x}", signature.s),
            "split_creator_bp": 9000,
            "split_admin_bp": 1000,
            "x402_contract": x402_contract,
            "creator_wallet": creator_wallet_string,
        });

        Ok(Invoice {
            provider: self.provider_key().to_string(),
            invoice_id: invoice_uid,
            payment_url: None,
            amount_cents: price_cents,
            currency: request.currency,
            status: PaymentStatus::Pending,
            raw,
        })
    }

    async fn confirm_payment(&self, _request: ConfirmPaymentRequest) -> Result<PaymentResult> {
        if !self.configured {
            bail!("x402 plugin is not configured. Missing env: {:?}", self.missing_env);
        }
        bail!("x402 plugin confirmation is not enabled yet. Use legacy POST /api/pay/x402/confirm for now")
    }
}
