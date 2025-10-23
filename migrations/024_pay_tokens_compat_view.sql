-- migrations/024_pay_tokens_compat_view.sql
CREATE OR REPLACE VIEW pay_tokens_compat AS
SELECT
  symbol,
  chain,
  chain_id,
  decimals,
  is_active,
  updated_at,
  -- expose both names; erc20_address is the canonical one
  erc20_address,
  erc20_address AS erc20
FROM pay_tokens;
