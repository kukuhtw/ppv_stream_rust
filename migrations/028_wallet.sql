-- 028_wallet.sql
-- Mini wallet: add balance to users, create wallet_transactions ledger

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'users' AND column_name = 'balance_cents'
  ) THEN
    ALTER TABLE users ADD COLUMN balance_cents BIGINT NOT NULL DEFAULT 0;
  END IF;
END $$;

CREATE TABLE IF NOT EXISTS wallet_transactions (
    id           BIGSERIAL PRIMARY KEY,
    user_id      TEXT        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    txn_type     TEXT        NOT NULL,  -- deposit | withdrawal | transfer_in | transfer_out
    amount_cents BIGINT      NOT NULL CHECK (amount_cents > 0),
    balance_after BIGINT     NOT NULL DEFAULT 0,
    status       TEXT        NOT NULL DEFAULT 'pending', -- pending | approved | completed | rejected
    ref_user_id  TEXT        REFERENCES users(id),       -- counterparty for transfers
    note         TEXT,                                    -- user-supplied description
    admin_note   TEXT,                                    -- admin approval/rejection note
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_wallet_txn_user') THEN
    CREATE INDEX idx_wallet_txn_user   ON wallet_transactions(user_id);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_wallet_txn_status') THEN
    CREATE INDEX idx_wallet_txn_status ON wallet_transactions(status);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_wallet_txn_type') THEN
    CREATE INDEX idx_wallet_txn_type   ON wallet_transactions(txn_type);
  END IF;
END $$;
