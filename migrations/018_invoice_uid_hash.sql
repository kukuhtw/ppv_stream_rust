-- migrations/018_invoice_uid_hash.sql
ALTER TABLE x402_invoices
  ADD COLUMN IF NOT EXISTS invoice_uid_hash TEXT,
  ADD COLUMN IF NOT EXISTS paid_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS payer_address TEXT,
  ADD COLUMN IF NOT EXISTS tx_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_x402_invoice_hash ON x402_invoices(invoice_uid_hash);
