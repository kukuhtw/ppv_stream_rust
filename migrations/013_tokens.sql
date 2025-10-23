-- 013_tokens.sql (FIXED)
CREATE TABLE IF NOT EXISTS pay_tokens (
  id SERIAL PRIMARY KEY,
  chain TEXT NOT NULL,             -- "ethereum","polygon","bsc", dll
  chain_id BIGINT NOT NULL,        -- 1,137,56, dst.
  symbol TEXT NOT NULL,            -- "ETH","USDT","USDC", ...
  decimals INT NOT NULL DEFAULT 18,
  erc20_address TEXT,              -- NULL untuk native coin
  is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- Buat unique index dengan COALESCE (bukan constraint inline)
CREATE UNIQUE INDEX IF NOT EXISTS idx_pay_tokens_unique
  ON pay_tokens (chain_id, COALESCE(erc20_address,''), symbol);