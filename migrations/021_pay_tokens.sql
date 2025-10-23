-- migrations/021_pay_tokens.sql (final, with unique constraint)
BEGIN;

-- ========== Tambah kolom yang mungkin belum ada ==========
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='erc20'
  ) THEN
    ALTER TABLE pay_tokens ADD COLUMN erc20 TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='decimals'
  ) THEN
    ALTER TABLE pay_tokens ADD COLUMN decimals INT DEFAULT 18;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='is_active'
  ) THEN
    ALTER TABLE pay_tokens ADD COLUMN is_active BOOLEAN DEFAULT TRUE;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='updated_at'
  ) THEN
    ALTER TABLE pay_tokens ADD COLUMN updated_at TIMESTAMP DEFAULT NOW();
  END IF;
END $$;

-- ========== Validasi format alamat ERC-20 ==========
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM   information_schema.check_constraints
    WHERE  constraint_name = 'pay_tokens_erc20_format_chk'
  ) THEN
    ALTER TABLE pay_tokens
    ADD CONSTRAINT pay_tokens_erc20_format_chk
    CHECK (erc20 IS NULL OR erc20 ~* '^0x[0-9a-f]{40}$');
  END IF;
END $$;

-- ========== Unique constraint untuk upsert ==========
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM   pg_constraint
    WHERE  conrelid = 'pay_tokens'::regclass
    AND    conname  = 'pay_tokens_chain_symbol_uq'
  ) THEN
    ALTER TABLE pay_tokens
      ADD CONSTRAINT pay_tokens_chain_symbol_uq UNIQUE (chain_id, symbol);
  END IF;
END $$;

-- ========== Trigger updated_at ==========
CREATE OR REPLACE FUNCTION trg_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_trigger WHERE tgname = 'set_timestamp_pay_tokens'
  ) THEN
    CREATE TRIGGER set_timestamp_pay_tokens
    BEFORE UPDATE ON pay_tokens
    FOR EACH ROW EXECUTE FUNCTION trg_set_timestamp();
  END IF;
END $$;

-- ========== Seed token default (gunakan ON CONFLICT ON CONSTRAINT) ==========
INSERT INTO pay_tokens (symbol, chain, chain_id, erc20, decimals, is_active)
VALUES ('MEGA', 'Mega Testnet', 6342, NULL, 18, TRUE)
ON CONFLICT ON CONSTRAINT pay_tokens_chain_symbol_uq DO UPDATE
SET chain     = EXCLUDED.chain,
    erc20     = EXCLUDED.erc20,
    decimals  = EXCLUDED.decimals,
    is_active = TRUE;

INSERT INTO pay_tokens (symbol, chain, chain_id, erc20, decimals, is_active)
VALUES ('USDC', 'Polygon', 137, '0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174', 6, TRUE)
ON CONFLICT ON CONSTRAINT pay_tokens_chain_symbol_uq DO UPDATE
SET chain     = EXCLUDED.chain,
    erc20     = EXCLUDED.erc20,
    decimals  = EXCLUDED.decimals,
    is_active = TRUE;

COMMIT;
