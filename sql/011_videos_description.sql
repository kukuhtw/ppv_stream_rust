-- 011_videos_description.sql
-- Tambah kolom description ke tabel videos (idempotent)

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_schema='public'
      AND table_name='videos'
      AND column_name='description'
  ) THEN
    ALTER TABLE videos
      ADD COLUMN description TEXT NOT NULL DEFAULT '';
  END IF;
END $$;

-- Optional: index kecil untuk pencarian full-text sederhana (judul+desc) via trigram (butuh pg_trgm)
-- CREATE EXTENSION IF NOT EXISTS pg_trgm;
-- CREATE INDEX IF NOT EXISTS idx_videos_title_desc_trgm ON videos
--   USING gin ((coalesce(title,'') || ' ' || coalesce(description,'')) gin_trgm_ops);
