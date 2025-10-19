-- 008_price_cents_bigint.sql (idempotent)

DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema='public'
      AND table_name='videos'
      AND column_name='price_cents'
      AND data_type <> 'bigint'
  ) THEN
    ALTER TABLE videos
      ALTER COLUMN price_cents TYPE BIGINT
      USING price_cents::bigint;
  END IF;
END $$;
