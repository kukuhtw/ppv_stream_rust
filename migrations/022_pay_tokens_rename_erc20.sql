-- migrations/022_pay_tokens_add_erc20_address.sql
BEGIN;

-- 1) Add erc20_address if missing
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='erc20_address'
  ) THEN
    ALTER TABLE pay_tokens ADD COLUMN erc20_address TEXT;
  END IF;
END $$;

-- 2) One-time backfill: copy from `erc20` if address is missing
UPDATE pay_tokens
   SET erc20_address = COALESCE(erc20_address, erc20)
 WHERE erc20_address IS DISTINCT FROM COALESCE(erc20, erc20_address);

-- 3) Ensure CHECK constraint is on erc20_address (drop old if present)
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.check_constraints
    WHERE constraint_name = 'pay_tokens_erc20_format_chk'
  ) THEN
    ALTER TABLE pay_tokens DROP CONSTRAINT pay_tokens_erc20_format_chk;
  END IF;

  ALTER TABLE pay_tokens
  ADD CONSTRAINT pay_tokens_erc20_format_chk
  CHECK (erc20_address IS NULL OR erc20_address ~* '^0x[0-9a-f]{40}$');
END $$;

-- 4) Reseed using erc20_address (idempotent)
INSERT INTO pay_tokens (symbol, chain, chain_id, erc20_address, decimals, is_active)
VALUES ('MEGA', 'Mega Testnet', 6342, NULL, 18, TRUE)
ON CONFLICT ON CONSTRAINT pay_tokens_chain_symbol_uq DO UPDATE
SET chain          = EXCLUDED.chain,
    erc20_address  = EXCLUDED.erc20_address,
    decimals       = EXCLUDED.decimals,
    is_active      = TRUE;

INSERT INTO pay_tokens (symbol, chain, chain_id, erc20_address, decimals, is_active)
VALUES ('USDC', 'Polygon', 137, '0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174', 6, TRUE)
ON CONFLICT ON CONSTRAINT pay_tokens_chain_symbol_uq DO UPDATE
SET chain          = EXCLUDED.chain,
    erc20_address  = EXCLUDED.erc20_address,
    decimals       = EXCLUDED.decimals,
    is_active      = TRUE;

COMMIT;
