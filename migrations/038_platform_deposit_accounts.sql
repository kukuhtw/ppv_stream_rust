-- 038_platform_deposit_accounts.sql
-- Platform-owned destination accounts for manual payments (wallet top-ups):
-- bank accounts and e-wallets (GoPay, OVO, DANA, ShopeePay, ...).
-- Managed by the admin from Settings; shown to logged-in users on the
-- wallet Deposit tab. Replaces the free-text WALLET_DEPOSIT_INSTRUCTIONS env
-- (which is still honoured as an extra note / fallback).
-- Idempotent: safe to run more than once.

CREATE TABLE IF NOT EXISTS platform_deposit_accounts (
    id             BIGSERIAL PRIMARY KEY,
    kind           TEXT        NOT NULL CHECK (kind IN ('bank', 'ewallet')),
    provider       TEXT        NOT NULL,          -- e.g. BCA, Mandiri, GoPay, OVO
    account_number TEXT        NOT NULL,          -- rekening / phone number
    account_holder TEXT        NOT NULL,          -- a.n.
    note           TEXT,                          -- e.g. "Include your username in the transfer note"
    is_active      BOOLEAN     NOT NULL DEFAULT TRUE,
    sort_order     INTEGER     NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_platform_deposit_accounts_active
    ON platform_deposit_accounts (is_active, sort_order, id);

-- Which platform account the user says they transferred to. The label is a
-- snapshot ("BCA 1234567890 a.n. PT Contoh") so the audit trail survives if
-- the account is later edited or deleted.
ALTER TABLE wallet_transactions
    ADD COLUMN IF NOT EXISTS deposit_account_id    BIGINT
        REFERENCES platform_deposit_accounts(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS deposit_account_label TEXT;
