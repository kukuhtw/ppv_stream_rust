-- 029_affiliate.sql
-- Affiliate system: creator sets per-video commission; affiliates earn on referral sales.

-- Per-video affiliate settings (configured by the video owner / User A)
CREATE TABLE IF NOT EXISTS affiliate_settings (
    video_id       TEXT    PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE,
    owner_id       TEXT    NOT NULL REFERENCES users(id),
    commission_pct INT     NOT NULL DEFAULT 0
        CHECK (commission_pct >= 0 AND commission_pct <= 90),
    is_enabled     BOOLEAN NOT NULL DEFAULT false,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Commission ledger: one row per successful referral sale
CREATE TABLE IF NOT EXISTS affiliate_commissions (
    id                   BIGSERIAL   PRIMARY KEY,
    video_id             TEXT        NOT NULL REFERENCES videos(id),
    affiliate_id         TEXT        NOT NULL REFERENCES users(id),  -- User B
    buyer_id             TEXT        NOT NULL REFERENCES users(id),  -- User C
    owner_id             TEXT        NOT NULL REFERENCES users(id),  -- User A
    purchase_price_cents BIGINT      NOT NULL,
    commission_cents     BIGINT      NOT NULL,
    payment_method       TEXT        NOT NULL DEFAULT 'wallet',      -- wallet | x402 | fiat
    ref_invoice_uid      TEXT,                                        -- links to invoice
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_aff_comm_affiliate') THEN
    CREATE INDEX idx_aff_comm_affiliate ON affiliate_commissions(affiliate_id);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_aff_comm_video') THEN
    CREATE INDEX idx_aff_comm_video     ON affiliate_commissions(video_id);
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_aff_comm_owner') THEN
    CREATE INDEX idx_aff_comm_owner     ON affiliate_commissions(owner_id);
  END IF;
END $$;

-- Add affiliate_ref column to existing invoice tables so the referrer is recorded
-- at invoice creation time and honoured at payment confirmation.
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'x402_invoices' AND column_name = 'affiliate_ref'
  ) THEN
    ALTER TABLE x402_invoices ADD COLUMN affiliate_ref TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'fiat_invoices' AND column_name = 'affiliate_ref'
  ) THEN
    ALTER TABLE fiat_invoices ADD COLUMN affiliate_ref TEXT;
  END IF;
END $$;
