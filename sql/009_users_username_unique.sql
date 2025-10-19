-- 009_users_username_unique.sql (idempotent)

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'users_username_unique'
  ) THEN
    ALTER TABLE users
      ADD CONSTRAINT users_username_unique UNIQUE (username);
  END IF;
END $$;
