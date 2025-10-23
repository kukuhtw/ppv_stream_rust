-- 019_x402_core.sql
-- Skema inti pembayaran X402 + profil kreator + kolom deskripsi video
-- Idempotent: aman dijalankan berulang (DO $$ ... $$ dan IF NOT EXISTS)
-- ============================================================
-- 4.b) Foreign keys untuk x402_invoices (idempotent & low-lock)
-- ============================================================
DO $$
BEGIN
  -- x402_invoices.user_id -> users(id)
  IF EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema='public' AND table_name='users'
  ) AND NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname='x402_inv_user_fk'
  ) THEN
    ALTER TABLE x402_invoices
      ADD CONSTRAINT x402_inv_user_fk
      FOREIGN KEY (user_id) REFERENCES users(id)
      ON UPDATE CASCADE ON DELETE RESTRICT
      NOT VALID;
    ALTER TABLE x402_invoices VALIDATE CONSTRAINT x402_inv_user_fk;
  END IF;

  -- x402_invoices.creator_id -> users(id)
  IF EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema='public' AND table_name='users'
  ) AND NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname='x402_inv_creator_fk'
  ) THEN
    ALTER TABLE x402_invoices
      ADD CONSTRAINT x402_inv_creator_fk
      FOREIGN KEY (creator_id) REFERENCES users(id)
      ON UPDATE CASCADE ON DELETE RESTRICT
      NOT VALID;
    ALTER TABLE x402_invoices VALIDATE CONSTRAINT x402_inv_creator_fk;
  END IF;

  -- x402_invoices.video_id -> videos(id)
  IF EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema='public' AND table_name='videos'
  ) AND NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname='x402_inv_video_fk'
  ) THEN
    ALTER TABLE x402_invoices
      ADD CONSTRAINT x402_inv_video_fk
      FOREIGN KEY (video_id) REFERENCES videos(id)
      ON UPDATE CASCADE ON DELETE RESTRICT
      NOT VALID;
    ALTER TABLE x402_invoices VALIDATE CONSTRAINT x402_inv_video_fk;
  END IF;
END
$$;

-- ============================================================
-- 5.b) Foreign key untuk purchases(video_id) -> videos(id)
-- ============================================================
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.tables
    WHERE table_schema='public' AND table_name='videos'
  ) AND NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname='purchases_video_fk'
  ) THEN
    ALTER TABLE purchases
      ADD CONSTRAINT purchases_video_fk
      FOREIGN KEY (video_id) REFERENCES videos(id)
      ON UPDATE CASCADE ON DELETE RESTRICT
      NOT VALID;
    ALTER TABLE purchases VALIDATE CONSTRAINT purchases_video_fk;
  END IF;
END
$$;
