//! Exchange rates and token price oracle.
//!
//! * `USD/IDR`   – rupiah per 1 USD. Used to charge Midtrans / Xendit in
//!   rupiah, to show rupiah prices, and to derive `price_cents` for videos
//!   priced in rupiah (`videos.price_currency = 'IDR'`).
//! * `<SYM>/USD` – USD per 1 token (e.g. `POL/USD`). Used by x402 to price
//!   native / volatile tokens. Stablecoins (USDC, USDT, DAI) are always 1:1
//!   and never need an oracle.
//!
//! Rates live in the `fx_rates` table (migration 037) and in an in-memory
//! cache so request handlers can read them synchronously. A background job
//! refreshes them from public sources; an admin can pin a rate with an
//! override, which the job then leaves untouched. Every change is written to
//! `fx_rate_history` for audit (ISO/IEC 27001 A.8.15).
//!
//! No personal data is sent to the rate sources: requests contain only the
//! currency / token identifiers.

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

pub const USD_IDR: &str = "USD/IDR";

/// Symbols priced 1:1 with USD (no oracle needed).
pub fn is_usd_stablecoin(symbol: &str, token_kind: Option<&str>) -> bool {
    if token_kind
        .map(|k| k.eq_ignore_ascii_case("STABLE"))
        .unwrap_or(false)
    {
        return true;
    }
    matches!(
        symbol.to_ascii_uppercase().as_str(),
        "USDC" | "USDC.E" | "USDT" | "DAI"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct Rate {
    pub pair: String,
    pub rate: f64,
    pub source: String,
    pub fetched_at: DateTime<Utc>,
    pub is_override: bool,
    pub updated_by: Option<String>,
}

impl Rate {
    pub fn age_secs(&self) -> i64 {
        (Utc::now() - self.fetched_at).num_seconds().max(0)
    }
}

static CACHE: Lazy<RwLock<HashMap<String, Rate>>> = Lazy::new(|| RwLock::new(HashMap::new()));

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|v: &f64| v.is_finite())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn cache_get(pair: &str) -> Option<Rate> {
    CACHE.read().ok()?.get(pair).cloned()
}

fn cache_put(rate: Rate) {
    if let Ok(mut map) = CACHE.write() {
        map.insert(rate.pair.clone(), rate);
    }
}

/// All cached rates, sorted by pair (for the admin page).
pub fn list() -> Vec<Rate> {
    let mut v: Vec<Rate> = CACHE
        .read()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default();
    v.sort_by(|a, b| a.pair.cmp(&b.pair));
    v
}

// ---------------------------------------------------------------------------
// Readers used by request handlers
// ---------------------------------------------------------------------------

/// Rupiah per USD. Uses the latest known rate (even if older than the refresh
/// interval — a slightly old rate is better than the static env value) and
/// falls back to `fallback` (DOLLAR_USD_TO_RUPIAH) when nothing is known.
pub fn usd_to_idr(fallback: f64) -> f64 {
    cache_get(USD_IDR)
        .map(|r| r.rate)
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(fallback)
}

/// Full USD/IDR record for display (`/api/kurs`).
pub fn usd_to_idr_info(fallback: f64) -> Rate {
    cache_get(USD_IDR).unwrap_or_else(|| Rate {
        pair: USD_IDR.into(),
        rate: fallback,
        source: "env".into(),
        fetched_at: Utc::now(),
        is_override: false,
        updated_by: None,
    })
}

/// True when the USD/IDR rate has not been refreshed for longer than
/// `FX_STALE_SECS` (default 48 h).
pub fn usd_to_idr_is_stale() -> bool {
    match cache_get(USD_IDR) {
        Some(r) if r.is_override => false,
        Some(r) => r.age_secs() as u64 > env_u64("FX_STALE_SECS", 172_800),
        None => true,
    }
}

/// Convert USD cents to whole rupiah at the current rate (rounded).
pub fn cents_to_idr(price_cents: i64, fallback: f64) -> i64 {
    ((price_cents as f64) * usd_to_idr(fallback) / 100.0).round() as i64
}

/// USD price of one `symbol` token, only when fresh enough to price a payment.
///
/// Oracle prices expire after `X402_ORACLE_MAX_AGE_SECS` (default 600 s).
/// Admin overrides expire after `FX_TOKEN_OVERRIDE_MAX_AGE_SECS` (default 24 h)
/// so a forgotten manual price cannot be used indefinitely.
pub fn token_usd(symbol: &str) -> Option<f64> {
    let pair = format!("{}/USD", symbol.to_ascii_uppercase());
    let r = cache_get(&pair)?;
    let max_age = if r.is_override {
        env_u64("FX_TOKEN_OVERRIDE_MAX_AGE_SECS", 86_400)
    } else {
        env_u64("X402_ORACLE_MAX_AGE_SECS", 600)
    };
    if r.age_secs() as u64 > max_age || !(r.rate.is_finite() && r.rate > 0.0) {
        return None;
    }
    Some(r.rate)
}

/// Rupiah price of a video when the creator priced it in rupiah.
pub async fn video_idr_price(pool: &PgPool, video_id: &str) -> Option<i64> {
    sqlx::query(
        "SELECT price_idr FROM videos WHERE id = $1 AND price_currency = 'IDR' AND price_idr IS NOT NULL",
    )
    .bind(video_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get::<i64, _>("price_idr").ok())
}

/// USD cents for a rupiah price at the current rate. Rounded up so the USD
/// charge is never worth less than the rupiah price; minimum 1 cent for a
/// non-zero price.
pub fn idr_to_cents(price_idr: i64, fallback: f64) -> i64 {
    if price_idr <= 0 {
        return 0;
    }
    let rate = usd_to_idr(fallback);
    ((price_idr as f64) * 100.0 / rate).ceil().max(1.0) as i64
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Load all rates into the cache. Seeds USD/IDR from the env value when the
/// table has no row yet.
pub async fn load_cache(pool: &PgPool, env_usd_idr: f64) {
    let rows =
        sqlx::query("SELECT pair, rate, source, fetched_at, is_override, updated_by FROM fx_rates")
            .fetch_all(pool)
            .await;
    match rows {
        Ok(rows) => {
            for r in rows {
                cache_put(Rate {
                    pair: r.try_get("pair").unwrap_or_default(),
                    rate: r.try_get("rate").unwrap_or(0.0),
                    source: r.try_get("source").unwrap_or_default(),
                    fetched_at: r.try_get("fetched_at").unwrap_or_else(|_| Utc::now()),
                    is_override: r.try_get("is_override").unwrap_or(false),
                    updated_by: r.try_get("updated_by").unwrap_or(None),
                });
            }
        }
        Err(e) => tracing::warn!("fx: could not load fx_rates (migration 037 applied?): {e}"),
    }
    if cache_get(USD_IDR).is_none() && env_usd_idr > 0.0 {
        // Old timestamp so the job fetches a live rate on its first tick.
        let seeded = Rate {
            pair: USD_IDR.into(),
            rate: env_usd_idr,
            source: "env".into(),
            fetched_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now),
            is_override: false,
            updated_by: None,
        };
        cache_put(seeded);
    }
}

async fn record_history(pool: &PgPool, r: &Rate) {
    let _ = sqlx::query(
        "INSERT INTO fx_rate_history (pair, rate, source, is_override, updated_by) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(&r.pair)
    .bind(r.rate)
    .bind(&r.source)
    .bind(r.is_override)
    .bind(&r.updated_by)
    .execute(pool)
    .await;
}

/// Store a rate fetched from a source. Never overwrites an admin override.
async fn store_fetched(pool: &PgPool, pair: &str, rate: f64, source: &str) -> bool {
    let res = sqlx::query(
        r#"INSERT INTO fx_rates (pair, rate, source, fetched_at, is_override, updated_by, updated_at)
           VALUES ($1, $2, $3, NOW(), FALSE, NULL, NOW())
           ON CONFLICT (pair) DO UPDATE
             SET rate = EXCLUDED.rate, source = EXCLUDED.source,
                 fetched_at = NOW(), updated_by = NULL, updated_at = NOW()
           WHERE fx_rates.is_override = FALSE
           RETURNING fetched_at"#,
    )
    .bind(pair)
    .bind(rate)
    .bind(source)
    .fetch_optional(pool)
    .await;
    match res {
        Ok(Some(row)) => {
            let r = Rate {
                pair: pair.into(),
                rate,
                source: source.into(),
                fetched_at: row.try_get("fetched_at").unwrap_or_else(|_| Utc::now()),
                is_override: false,
                updated_by: None,
            };
            record_history(pool, &r).await;
            cache_put(r);
            true
        }
        Ok(None) => false, // override active
        Err(e) => {
            tracing::warn!(pair, "fx: store failed: {e}");
            false
        }
    }
}

/// Re-derive `price_cents` for videos priced in rupiah.
pub async fn reprice_idr_videos(pool: &PgPool, fallback: f64) {
    let rate = usd_to_idr(fallback);
    if !(rate.is_finite() && rate > 0.0) {
        return;
    }
    let res = sqlx::query(
        r#"UPDATE videos
           SET price_cents = CASE WHEN price_idr <= 0 THEN 0
                                  ELSE GREATEST(1, CEIL(price_idr * 100.0 / $1))::BIGINT END
           WHERE price_currency = 'IDR' AND price_idr IS NOT NULL
             AND price_cents IS DISTINCT FROM
                 (CASE WHEN price_idr <= 0 THEN 0
                       ELSE GREATEST(1, CEIL(price_idr * 100.0 / $1))::BIGINT END)"#,
    )
    .bind(rate)
    .execute(pool)
    .await;
    match res {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!(rate, updated = r.rows_affected(), "fx: repriced IDR videos")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("fx: reprice IDR videos failed: {e}"),
    }
}

/// Validate a pair name such as `USD/IDR` or `POL/USD`.
pub fn valid_pair(pair: &str) -> bool {
    if pair == USD_IDR {
        return true;
    }
    match pair.split_once('/') {
        Some((sym, "USD")) => {
            (2..=12).contains(&sym.len())
                && sym.starts_with(|c: char| c.is_ascii_uppercase())
                && sym
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '.')
        }
        _ => false,
    }
}

fn usd_idr_bounds() -> (f64, f64) {
    (
        env_f64("FX_USD_IDR_MIN", 5_000.0),
        env_f64("FX_USD_IDR_MAX", 50_000.0),
    )
}

/// Admin override: pin `pair` to `rate` until cleared.
pub async fn set_override(
    pool: &PgPool,
    pair: &str,
    rate: f64,
    admin_id: &str,
    env_usd_idr: f64,
) -> Result<Rate, String> {
    if !valid_pair(pair) {
        return Err("pair must be USD/IDR or <SYMBOL>/USD".into());
    }
    if !(rate.is_finite() && rate > 0.0) {
        return Err("rate must be a positive number".into());
    }
    if pair == USD_IDR {
        let (lo, hi) = usd_idr_bounds();
        if rate < lo || rate > hi {
            return Err(format!("USD/IDR must be between {lo} and {hi}"));
        }
    }
    let row = sqlx::query(
        r#"INSERT INTO fx_rates (pair, rate, source, fetched_at, is_override, updated_by, updated_at)
           VALUES ($1, $2, 'admin', NOW(), TRUE, $3, NOW())
           ON CONFLICT (pair) DO UPDATE
             SET rate = EXCLUDED.rate, source = 'admin', fetched_at = NOW(),
                 is_override = TRUE, updated_by = EXCLUDED.updated_by, updated_at = NOW()
           RETURNING fetched_at"#,
    )
    .bind(pair)
    .bind(rate)
    .bind(admin_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("db: {e}"))?;
    let r = Rate {
        pair: pair.into(),
        rate,
        source: "admin".into(),
        fetched_at: row.try_get("fetched_at").unwrap_or_else(|_| Utc::now()),
        is_override: true,
        updated_by: Some(admin_id.into()),
    };
    record_history(pool, &r).await;
    cache_put(r.clone());
    if pair == USD_IDR {
        reprice_idr_videos(pool, env_usd_idr).await;
    }
    tracing::info!(
        pair,
        rate,
        admin_id,
        action = "fx_override_set",
        "fx override set"
    );
    Ok(r)
}

/// Remove an admin override; the next job tick fetches a live value.
pub async fn clear_override(pool: &PgPool, pair: &str, admin_id: &str) -> Result<(), String> {
    if !valid_pair(pair) {
        return Err("invalid pair".into());
    }
    sqlx::query(
        r#"UPDATE fx_rates
           SET is_override = FALSE, updated_by = $2, updated_at = NOW(),
               fetched_at = TIMESTAMPTZ 'epoch'
           WHERE pair = $1"#,
    )
    .bind(pair)
    .bind(admin_id)
    .execute(pool)
    .await
    .map_err(|e| format!("db: {e}"))?;
    if let Some(mut r) = cache_get(pair) {
        r.is_override = false;
        r.updated_by = Some(admin_id.into());
        r.fetched_at = DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now);
        cache_put(r);
    }
    tracing::info!(
        pair,
        admin_id,
        action = "fx_override_cleared",
        "fx override cleared"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Background refresh job
// ---------------------------------------------------------------------------

fn http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("ppv-stream-fx/1.0")
        .build()
        .ok()
}

/// Accept a new value only if it is within `max_pct` of the previous one.
/// Protects against a broken or malicious source moving prices wildly.
fn within_change(prev: Option<&Rate>, new: f64, max_pct: f64) -> bool {
    match prev {
        Some(p) if p.rate > 0.0 && p.source != "env" => {
            ((new - p.rate).abs() / p.rate) * 100.0 <= max_pct
        }
        _ => true,
    }
}

async fn refresh_usd_idr(pool: &PgPool, client: &reqwest::Client, env_usd_idr: f64) {
    let url = std::env::var("FX_USD_IDR_URL")
        .unwrap_or_else(|_| "https://open.er-api.com/v6/latest/USD".into());
    let pointer = std::env::var("FX_USD_IDR_JSON_POINTER").unwrap_or_else(|_| "/rates/IDR".into());
    let body: serde_json::Value = match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("fx: USD/IDR parse failed: {e}");
                return;
            }
        },
        Ok(resp) => {
            tracing::warn!("fx: USD/IDR source returned HTTP {}", resp.status());
            return;
        }
        Err(e) => {
            tracing::warn!("fx: USD/IDR fetch failed: {e}");
            return;
        }
    };
    let Some(rate) = body.pointer(&pointer).and_then(|v| v.as_f64()) else {
        tracing::warn!(pointer, "fx: USD/IDR value not found in response");
        return;
    };
    let (lo, hi) = usd_idr_bounds();
    if !(rate.is_finite() && rate >= lo && rate <= hi) {
        tracing::warn!(rate, "fx: USD/IDR outside sanity bounds; ignored");
        return;
    }
    let prev = cache_get(USD_IDR);
    if !within_change(prev.as_ref(), rate, env_f64("FX_MAX_CHANGE_PCT", 10.0)) {
        tracing::warn!(
            rate,
            prev = prev.map(|p| p.rate),
            "fx: USD/IDR change larger than FX_MAX_CHANGE_PCT; ignored (set an admin override if genuine)"
        );
        return;
    }
    if store_fetched(pool, USD_IDR, rate, "open.er-api.com").await {
        reprice_idr_videos(pool, env_usd_idr).await;
    }
}

/// `SYMBOL=coingecko-id` pairs from `FX_TOKEN_IDS`.
fn token_id_map() -> HashMap<String, String> {
    let raw = std::env::var("FX_TOKEN_IDS").unwrap_or_else(|_| {
        "POL=polygon-ecosystem-token,MATIC=polygon-ecosystem-token,ETH=ethereum,WETH=ethereum,BNB=binancecoin,AVAX=avalanche-2".into()
    });
    raw.split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_uppercase(), v.trim().to_string()))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect()
}

/// Symbols of active, non-stablecoin x402 tokens.
async fn active_volatile_symbols(pool: &PgPool) -> Vec<String> {
    let rows = sqlx::query(
        "SELECT DISTINCT UPPER(symbol) AS symbol, token_kind::text AS token_kind FROM pay_tokens WHERE is_active = TRUE",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.iter()
        .filter_map(|r| {
            let sym: String = r.try_get("symbol").ok()?;
            let kind: Option<String> = r.try_get("token_kind").ok().flatten();
            (!is_usd_stablecoin(&sym, kind.as_deref())).then_some(sym)
        })
        .collect()
}

async fn refresh_tokens(pool: &PgPool, client: &reqwest::Client) {
    let ids = token_id_map();
    let refresh_secs = env_u64("FX_TOKEN_REFRESH_SECS", 120);
    let wanted: Vec<(String, String)> = active_volatile_symbols(pool)
        .await
        .into_iter()
        .filter_map(|sym| ids.get(&sym).map(|id| (sym, id.clone())))
        .filter(|(sym, _)| match cache_get(&format!("{sym}/USD")) {
            Some(r) => !r.is_override && r.age_secs() as u64 >= refresh_secs,
            None => true,
        })
        .collect();
    if wanted.is_empty() {
        return;
    }
    let mut id_list: Vec<&str> = wanted.iter().map(|(_, id)| id.as_str()).collect();
    id_list.sort_unstable();
    id_list.dedup();
    let base = std::env::var("FX_COINGECKO_URL")
        .unwrap_or_else(|_| "https://api.coingecko.com/api/v3/simple/price".into());
    let mut req = client
        .get(&base)
        .query(&[("ids", id_list.join(",")), ("vs_currencies", "usd".into())]);
    if let Ok(key) = std::env::var("COINGECKO_API_KEY") {
        if !key.trim().is_empty() {
            req = req.header("x-cg-demo-api-key", key.trim());
        }
    }
    let body: serde_json::Value = match req.send().await {
        Ok(resp) if resp.status().is_success() => match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("fx: token price parse failed: {e}");
                return;
            }
        },
        Ok(resp) => {
            tracing::warn!("fx: token price source returned HTTP {}", resp.status());
            return;
        }
        Err(e) => {
            tracing::warn!("fx: token price fetch failed: {e}");
            return;
        }
    };
    let max_change = env_f64("FX_TOKEN_MAX_CHANGE_PCT", 30.0);
    for (sym, id) in wanted {
        let Some(price) = body.pointer(&format!("/{id}/usd")).and_then(|v| v.as_f64()) else {
            tracing::warn!(%sym, %id, "fx: no USD price in oracle response");
            continue;
        };
        if !(price.is_finite() && price > 0.0) {
            continue;
        }
        let pair = format!("{sym}/USD");
        let prev = cache_get(&pair);
        // Compare only against a recent value; an old one may legitimately differ a lot.
        let prev_recent = prev.filter(|p| p.age_secs() < 3_600);
        if !within_change(prev_recent.as_ref(), price, max_change) {
            tracing::warn!(%pair, price, "fx: token price jump larger than FX_TOKEN_MAX_CHANGE_PCT; ignored");
            continue;
        }
        store_fetched(pool, &pair, price, "coingecko").await;
    }
}

/// Spawn the refresh job. Disable with `FX_AUTO_REFRESH=0` (rates then only
/// change through admin overrides).
pub fn start_fx_worker(pool: PgPool, env_usd_idr: f64) {
    if !env_bool("FX_AUTO_REFRESH", true) {
        tracing::info!("fx: auto refresh disabled (FX_AUTO_REFRESH=0)");
        return;
    }
    tokio::spawn(async move {
        let Some(client) = http_client() else {
            tracing::error!("fx: could not build HTTP client; refresh disabled");
            return;
        };
        let idr_every = env_u64("FX_REFRESH_SECS", 3_600);
        loop {
            let idr_due = match cache_get(USD_IDR) {
                Some(r) => !r.is_override && r.age_secs() as u64 >= idr_every,
                None => true,
            };
            if idr_due {
                refresh_usd_idr(&pool, &client, env_usd_idr).await;
            }
            refresh_tokens(&pool, &client).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

// ---------------------------------------------------------------------------
// Token amount helper for x402
// ---------------------------------------------------------------------------

/// How a token amount was priced.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenPricing {
    /// USD stablecoin, 1 token = 1 USD.
    Stable,
    /// Oracle price (USD per token).
    Oracle(f64),
    /// Testnet-only 1:1 fallback (X402_ALLOW_VOLATILE_TOKENS=1, no oracle).
    UnpricedFallback,
}

/// Token base units for `price_cents`, rounded up, plus a display string.
/// Returns `None` when the token cannot be priced safely.
pub fn x402_token_amount(
    price_cents: i64,
    symbol: &str,
    token_kind: Option<&str>,
    decimals: u32,
) -> Option<(u128, String, TokenPricing)> {
    if price_cents <= 0 || decimals > 30 {
        return None;
    }
    let sym = symbol.to_string();
    if is_usd_stablecoin(symbol, token_kind) {
        let wei = (price_cents as u128)
            .checked_mul(10u128.checked_pow(decimals)?)?
            .checked_add(99)?
            / 100;
        let disp = format!(
            "{}.{:02} {}",
            price_cents / 100,
            (price_cents % 100).unsigned_abs(),
            sym
        );
        return Some((wei, disp, TokenPricing::Stable));
    }
    if let Some(usd_per_token) = token_usd(symbol) {
        let tokens = (price_cents as f64 / 100.0) / usd_per_token;
        let wei = token_units_ceil(tokens, decimals)?;
        let disp = format!("{} {}", format_units(wei, decimals, 6), sym);
        return Some((wei, disp, TokenPricing::Oracle(usd_per_token)));
    }
    if env_bool("X402_ALLOW_VOLATILE_TOKENS", false) {
        let wei = (price_cents as u128)
            .checked_mul(10u128.checked_pow(decimals)?)?
            .checked_add(99)?
            / 100;
        let disp = format!(
            "{}.{:02} {} (testnet 1:1)",
            price_cents / 100,
            (price_cents % 100).unsigned_abs(),
            sym
        );
        return Some((wei, disp, TokenPricing::UnpricedFallback));
    }
    None
}

/// `tokens` → base units, rounded up to 6 significant decimals of the token.
fn token_units_ceil(tokens: f64, decimals: u32) -> Option<u128> {
    if !(tokens.is_finite() && tokens > 0.0) {
        return None;
    }
    let shown = decimals.min(6);
    // Whole micro-units (or fewer decimals for low-decimal tokens), rounded up.
    let micro = (tokens * 10f64.powi(shown as i32)).ceil();
    if !(micro.is_finite() && micro >= 1.0 && micro < 1e30) {
        return None;
    }
    (micro as u128).checked_mul(10u128.checked_pow(decimals - shown)?)
}

/// Format base units with at most `max_frac` fractional digits (trailing zeros trimmed).
fn format_units(wei: u128, decimals: u32, max_frac: u32) -> String {
    let unit = 10u128.pow(decimals);
    let whole = wei / unit;
    let frac = wei % unit;
    if decimals == 0 {
        return whole.to_string();
    }
    let frac_str = format!("{:0width$}", frac, width = decimals as usize);
    let cut: String = frac_str.chars().take(max_frac as usize).collect();
    let trimmed = cut.trim_end_matches('0');
    if trimmed.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(pair: &str, rate: f64, age_secs: i64, is_override: bool) {
        cache_put(Rate {
            pair: pair.into(),
            rate,
            source: "test".into(),
            fetched_at: Utc::now() - chrono::Duration::seconds(age_secs),
            is_override,
            updated_by: None,
        });
    }

    #[test]
    fn stablecoin_is_one_to_one() {
        let (wei, disp, p) = x402_token_amount(500, "USDC", None, 6).unwrap();
        assert_eq!(wei, 5_000_000);
        assert_eq!(disp, "5.00 USDC");
        assert_eq!(p, TokenPricing::Stable);
    }

    #[test]
    fn oracle_prices_native_token_rounding_up() {
        put("TSTA/USD", 0.25, 10, false); // 1 TSTA = $0.25
        let (wei, disp, p) = x402_token_amount(500, "TSTA", None, 18).unwrap();
        assert_eq!(wei, 20 * 10u128.pow(18)); // $5 / 0.25 = 20 tokens
        assert_eq!(disp, "20 TSTA");
        assert_eq!(p, TokenPricing::Oracle(0.25));

        put("TSTB/USD", 3.0, 10, false); // $1 / 3 = 0.333334 (rounded up)
        let (wei, disp, _) = x402_token_amount(100, "TSTB", None, 18).unwrap();
        assert_eq!(wei, 333_334 * 10u128.pow(12));
        assert_eq!(disp, "0.333334 TSTB");
    }

    #[test]
    fn stale_oracle_price_is_refused() {
        put("TSTC/USD", 1.0, 100_000, false);
        assert!(token_usd("TSTC").is_none());
        put("TSTD/USD", 1.0, 3_600, true); // override valid for 24h
        assert!(token_usd("TSTD").is_some());
    }

    #[test]
    fn idr_conversion_rounds_up() {
        put(USD_IDR, 16_000.0, 10, false);
        assert_eq!(idr_to_cents(50_000, 1.0), 313); // 312.5 → 313
        assert_eq!(idr_to_cents(0, 1.0), 0);
        assert_eq!(cents_to_idr(500, 1.0), 80_000);
    }

    #[test]
    fn pair_validation() {
        assert!(valid_pair("USD/IDR"));
        assert!(valid_pair("POL/USD"));
        assert!(valid_pair("USDC.E/USD"));
        assert!(!valid_pair("pol/usd"));
        assert!(!valid_pair("POL/IDR"));
        assert!(!valid_pair("../USD"));
    }

    #[test]
    fn change_guard() {
        let prev = Rate {
            pair: USD_IDR.into(),
            rate: 16_000.0,
            source: "x".into(),
            fetched_at: Utc::now(),
            is_override: false,
            updated_by: None,
        };
        assert!(within_change(Some(&prev), 16_500.0, 10.0));
        assert!(!within_change(Some(&prev), 20_000.0, 10.0));
        let env_prev = Rate {
            source: "env".into(),
            ..prev
        };
        assert!(within_change(Some(&env_prev), 20_000.0, 10.0));
    }
}
