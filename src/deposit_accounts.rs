// src/deposit_accounts.rs
//
// Platform-owned destination accounts for manual payments: bank accounts and
// e-wallets that users transfer to when topping up their wallet. The admin
// manages them from Settings (see migrations/038_platform_deposit_accounts.sql).

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

pub const KINDS: [&str; 2] = ["bank", "ewallet"];

#[derive(Clone, Debug, Serialize)]
pub struct DepositAccount {
    pub id: i64,
    pub kind: String,
    pub provider: String,
    pub account_number: String,
    pub account_holder: String,
    pub note: Option<String>,
    pub is_active: bool,
    pub sort_order: i32,
}

impl DepositAccount {
    /// Human-readable snapshot stored on the deposit transaction.
    pub fn label(&self) -> String {
        format!(
            "{} {} a.n. {}",
            self.provider, self.account_number, self.account_holder
        )
    }
}

/// Admin input for create / update.
#[derive(Debug, Deserialize)]
pub struct DepositAccountInput {
    pub kind: String,
    pub provider: String,
    pub account_number: String,
    pub account_holder: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_true() -> bool {
    true
}

/// Validated, trimmed input ready to be written.
pub struct CleanInput {
    pub kind: String,
    pub provider: String,
    pub account_number: String,
    pub account_holder: String,
    pub note: Option<String>,
    pub is_active: bool,
    pub sort_order: i32,
}

fn clip(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

impl DepositAccountInput {
    pub fn validate(&self) -> Result<CleanInput, &'static str> {
        let kind = self.kind.trim().to_ascii_lowercase();
        if !KINDS.contains(&kind.as_str()) {
            return Err("kind must be 'bank' or 'ewallet'");
        }
        let provider = clip(&self.provider, 60);
        let account_number = clip(&self.account_number, 60);
        let account_holder = clip(&self.account_holder, 120);
        if provider.is_empty() || account_number.is_empty() || account_holder.is_empty() {
            return Err("provider, account number and account holder are required");
        }
        // Account numbers / e-wallet phone numbers: digits plus common separators.
        if !account_number
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+' | '.'))
            || !account_number.chars().any(|c| c.is_ascii_digit())
        {
            return Err("account number may only contain digits, spaces, '-', '+' or '.'");
        }
        let note = self
            .note
            .as_deref()
            .map(|n| clip(n, 300))
            .filter(|n| !n.is_empty());
        Ok(CleanInput {
            kind,
            provider,
            account_number,
            account_holder,
            note,
            is_active: self.is_active,
            sort_order: self.sort_order.clamp(-1000, 1000),
        })
    }
}

fn from_row(r: &sqlx::postgres::PgRow) -> DepositAccount {
    DepositAccount {
        id: r.try_get("id").unwrap_or(0),
        kind: r.try_get("kind").unwrap_or_default(),
        provider: r.try_get("provider").unwrap_or_default(),
        account_number: r.try_get("account_number").unwrap_or_default(),
        account_holder: r.try_get("account_holder").unwrap_or_default(),
        note: r.try_get("note").unwrap_or(None),
        is_active: r.try_get("is_active").unwrap_or(false),
        sort_order: r.try_get("sort_order").unwrap_or(0),
    }
}

const COLS: &str =
    "id, kind, provider, account_number, account_holder, note, is_active, sort_order";

/// All accounts (admin view). Returns empty when the table is missing.
pub async fn list_all(pool: &PgPool) -> Vec<DepositAccount> {
    sqlx::query(&format!(
        "SELECT {COLS} FROM platform_deposit_accounts ORDER BY sort_order, id"
    ))
    .fetch_all(pool)
    .await
    .map(|rows| rows.iter().map(from_row).collect())
    .unwrap_or_default()
}

/// Active accounts shown to users.
pub async fn list_active(pool: &PgPool) -> Vec<DepositAccount> {
    sqlx::query(&format!(
        "SELECT {COLS} FROM platform_deposit_accounts WHERE is_active ORDER BY sort_order, id"
    ))
    .fetch_all(pool)
    .await
    .map(|rows| rows.iter().map(from_row).collect())
    .unwrap_or_default()
}

pub async fn get_active(pool: &PgPool, id: i64) -> Option<DepositAccount> {
    sqlx::query(&format!(
        "SELECT {COLS} FROM platform_deposit_accounts WHERE id = $1 AND is_active"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|r| from_row(&r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: &str, number: &str) -> DepositAccountInput {
        DepositAccountInput {
            kind: kind.into(),
            provider: " BCA ".into(),
            account_number: number.into(),
            account_holder: "PT Contoh".into(),
            note: Some("   ".into()),
            is_active: true,
            sort_order: 5000,
        }
    }

    #[test]
    fn validates_bank_and_ewallet() {
        let c = input("Bank", "123-456 7890").validate().unwrap();
        assert_eq!(c.kind, "bank");
        assert_eq!(c.provider, "BCA");
        assert_eq!(c.note, None);
        assert_eq!(c.sort_order, 1000);
        assert!(input("ewallet", "+62 812-0000-0000").validate().is_ok());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(input("crypto", "123").validate().is_err());
        assert!(input("bank", "abc").validate().is_err());
        assert!(input("bank", "--").validate().is_err());
        assert!(input("bank", "").validate().is_err());
    }
}
