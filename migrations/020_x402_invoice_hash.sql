-- migrations/020_x402_invoice_hash.sql
-- Tambah kolom invoice_uid_hash untuk pencocokan event on-chain
-- Aman dijalankan berulang kali

-- 1) Tambah kolom (nullable dulu agar tidak lock lama saat ada data)
ALTER TABLE x402_invoices
  ADD COLUMN IF NOT EXISTS invoice_uid_hash TEXT;

-- 2) Buat partial unique index (hanya untuk baris yang sudah terisi hash)
--    Kenapa partial? Agar data lama yang NULL tidak melanggar uniq.
CREATE UNIQUE INDEX IF NOT EXISTS x402_inv_uid_hash_uq
  ON x402_invoices (invoice_uid_hash)
  WHERE invoice_uid_hash IS NOT NULL;

-- (Opsional) Index pencarian campuran jika sering query by (video_id,status)
CREATE INDEX IF NOT EXISTS x402_inv_video_status_idx
  ON x402_invoices (video_id, status);

-- 3) (Opsional) Backfill sementara dengan SHA256 kalau pgcrypto tersedia,
--    HANYA sebagai placeholder supaya tidak NULL.
--    NB: Kode aplikasi memakai Keccak256; untuk valid on-chain matching,
--    backfill ke Keccak256 sebaiknya dilakukan oleh aplikasi/job terpisah.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_extension WHERE extname = 'pgcrypto'
  ) THEN
    UPDATE x402_invoices
       SET invoice_uid_hash = '0x' || encode(digest(invoice_uid, 'sha256'), 'hex')
     WHERE invoice_uid_hash IS NULL;
  END IF;
END
$$;

-- 4) (Opsional, setelah Anda yakin semua baris sudah berisi hash Keccak256):
-- ALTER TABLE x402_invoices
--   ALTER COLUMN invoice_uid_hash SET NOT NULL;
