-- migrations/023_x402_underpay_and_quote.sql
CREATE EXTENSION IF NOT EXISTS pgcrypto; -- for gen_random_uuid()

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='x402_invoices' AND column_name='required_amount_wei'
  ) THEN
    ALTER TABLE x402_invoices ADD COLUMN required_amount_wei NUMERIC;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='x402_invoices' AND column_name='paid_amount_wei'
  ) THEN
    ALTER TABLE x402_invoices ADD COLUMN paid_amount_wei NUMERIC DEFAULT 0;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='x402_invoices' AND column_name='invoice_group_uid'
  ) THEN
    ALTER TABLE x402_invoices ADD COLUMN invoice_group_uid UUID;
    UPDATE x402_invoices
       SET invoice_group_uid = gen_random_uuid()
     WHERE invoice_group_uid IS NULL;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name='x402_invoices' AND column_name='expires_at'
  ) THEN
    ALTER TABLE x402_invoices ADD COLUMN expires_at TIMESTAMPTZ;
  END IF;
END $$;

-- Consistent status values (add once)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.check_constraints
    WHERE constraint_name = 'x402_invoices_status_chk'
  ) THEN
    ALTER TABLE x402_invoices
      ADD CONSTRAINT x402_invoices_status_chk
      CHECK (status IN ('pending','paid','underpaid','expired','cancelled'));
  END IF;
END $$;

-- Helpful index for top-up lookups
CREATE INDEX IF NOT EXISTS x402_inv_group_idx
  ON x402_invoices (invoice_group_uid);
