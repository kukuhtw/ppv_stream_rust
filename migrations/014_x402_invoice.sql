-- 014_x402_invoice.sql
CREATE TABLE IF NOT EXISTS x402_invoices (
  id BIGSERIAL PRIMARY KEY,
  invoice_uid TEXT NOT NULL UNIQUE,     -- UUID v4
  user_id TEXT NOT NULL REFERENCES users(id),
  video_id TEXT NOT NULL REFERENCES videos(id),
  creator_id TEXT NOT NULL REFERENCES users(id),
  chain_id BIGINT NOT NULL,
  token_symbol TEXT NOT NULL,
  token_address TEXT,                   -- NULL untuk native
  price_cents BIGINT NOT NULL,          -- dari videos.price_cents (USD cents)
  usd_to_idr NUMERIC(18,2),             -- kurs saat invoicing (opsional)
  token_amount NUMERIC(78,0) NOT NULL,  -- dalam "wei" (10^decimals)
  split_creator_bp INT NOT NULL DEFAULT 9000, -- 9000 = 90.00%
  split_admin_bp INT NOT NULL DEFAULT 1000,
  status TEXT NOT NULL DEFAULT 'pending',     -- pending|paid|expired|cancelled
  payer_address TEXT,                     -- terisi saat bayar
  tx_hash TEXT,                           -- tx konfirmasi on-chain
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  paid_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_x402_invoice_uid ON x402_invoices(invoice_uid);
CREATE INDEX IF NOT EXISTS idx_x402_invoice_status ON x402_invoices(status);
CREATE INDEX IF NOT EXISTS idx_x402_invoice_video ON x402_invoices(video_id);
