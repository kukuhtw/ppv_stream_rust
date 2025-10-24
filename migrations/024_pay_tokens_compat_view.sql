-- migrations/024_pay_tokens_compat_view.sql
BEGIN;

-- Hapus view lama (kalau ada) supaya kita bisa mengubah daftar kolomnya
DROP VIEW IF EXISTS pay_tokens_compat;

-- Buat ulang view dengan kolom yang diinginkan
-- Catatan: kita ekspose dua nama untuk kompatibilitas kode lama:
--  - erc20_address (kanonik)
--  - erc20         (alias dari erc20_address)
CREATE VIEW pay_tokens_compat AS
SELECT
  symbol,
  chain,
  chain_id,
  decimals,
  is_active,
  updated_at,
  erc20_address       AS erc20_address,
  erc20_address       AS erc20
FROM pay_tokens;

COMMIT;
