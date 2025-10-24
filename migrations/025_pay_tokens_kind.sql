-- migrations/025_pay_tokens_kind.sql
BEGIN;

-- ============================================================
-- 1) ENUM pay_token_kind
-- ============================================================
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_type WHERE typname = 'pay_token_kind'
  ) THEN
    CREATE TYPE pay_token_kind AS ENUM ('NATIVE','ERC20','STABLE');
  END IF;
END $$;

-- ============================================================
-- 2) Tambah kolom token_kind (jika belum ada)
-- ============================================================
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='token_kind'
  ) THEN
    ALTER TABLE pay_tokens
      ADD COLUMN token_kind pay_token_kind;
  END IF;
END $$;

-- ============================================================
-- 3) Inisialisasi nilai token_kind
--     - erc20_address IS NULL  -> NATIVE
--     - erc20_address IS NOT NULL -> ERC20
-- ============================================================
UPDATE pay_tokens
SET token_kind = CASE
  WHEN erc20_address IS NULL THEN 'NATIVE'::pay_token_kind
  ELSE 'ERC20'::pay_token_kind
END
WHERE token_kind IS NULL;

-- ============================================================
-- 4) Tandai stablecoin populer sebagai STABLE
-- ============================================================
UPDATE pay_tokens
SET token_kind = 'STABLE'::pay_token_kind
WHERE UPPER(symbol) IN ('USDC','USDT','DAI','USDP','TUSD','FDUSD')
  AND erc20_address IS NOT NULL;

-- ============================================================
-- 5) Constraint konsistensi token_kind ↔ erc20_address
-- ============================================================
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM   pg_constraint
    WHERE  conrelid = 'pay_tokens'::regclass
    AND    conname  = 'pay_tokens_kind_erc20_chk'
  ) THEN
    ALTER TABLE pay_tokens
    ADD CONSTRAINT pay_tokens_kind_erc20_chk
    CHECK (
      (token_kind = 'NATIVE' AND erc20_address IS NULL)
      OR
      (token_kind IN ('ERC20','STABLE') AND erc20_address IS NOT NULL)
    );
  END IF;
END $$;

-- ============================================================
-- 6) Kolom turunan is_native (BOOLEAN GENERATED ALWAYS)
-- ============================================================
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='pay_tokens' AND column_name='is_native'
  ) THEN
    ALTER TABLE pay_tokens
      ADD COLUMN is_native BOOLEAN GENERATED ALWAYS AS (token_kind = 'NATIVE') STORED;
  END IF;
END $$;

-- ============================================================
-- 7) Koreksi nilai token_kind untuk jaringan spesifik
-- ============================================================
-- Mega Testnet
UPDATE pay_tokens
SET token_kind = 'NATIVE'::pay_token_kind
WHERE chain_id = 6342 AND UPPER(symbol) = 'MEGA';

-- Polygon Mainnet
UPDATE pay_tokens
SET token_kind = 'STABLE'::pay_token_kind
WHERE chain_id = 137 AND UPPER(symbol) = 'USDC';

-- Polygon Amoy Testnet (jika ada data)
UPDATE pay_tokens
SET token_kind = 'NATIVE'::pay_token_kind
WHERE chain_id = 80002 AND UPPER(symbol) IN ('POL','MATIC');

-- ============================================================
-- 8) View kompatibilitas (untuk kode lama)
-- ============================================================
CREATE OR REPLACE VIEW pay_tokens_compat AS
SELECT
  symbol,
  chain,
  chain_id,
  decimals,
  is_active,
  updated_at,
  erc20_address       AS erc20_address,
  erc20_address       AS erc20,   -- alias untuk kompatibilitas kode lama
  token_kind,
  is_native
FROM pay_tokens;

COMMIT;
