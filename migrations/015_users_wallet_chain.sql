-- 015_users_wallet_chain.sql (profil kreator)
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name='users' AND column_name='wallet_chain_id') THEN
    ALTER TABLE users ADD COLUMN wallet_chain_id BIGINT; -- preferensi chain kreator
  END IF;
END $$;
