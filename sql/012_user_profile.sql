-- 012_user_profile.sql
-- Tambah kolom profil & kontak pengguna (idempotent)

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema='public' AND table_name='users' AND column_name='bank_account'
  ) THEN
    ALTER TABLE users ADD COLUMN bank_account TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema='public' AND table_name='users' AND column_name='wallet_account'
  ) THEN
    ALTER TABLE users ADD COLUMN wallet_account TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema='public' AND table_name='users' AND column_name='whatsapp'
  ) THEN
    ALTER TABLE users ADD COLUMN whatsapp TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema='public' AND table_name='users' AND column_name='profile_desc'
  ) THEN
    ALTER TABLE users ADD COLUMN profile_desc TEXT NOT NULL DEFAULT '';
  END IF;

  -- Opsional: email sudah ada dan UNIQUE; user boleh update melalui /api/profile_update.
END $$;

-- (opsional) index kecil utk pencarian WA / wallet
CREATE INDEX IF NOT EXISTS idx_users_whatsapp ON users(whatsapp);
