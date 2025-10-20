-- 010_videos_hls.sql
-- Tambahan kolom metadata HLS untuk tabel videos
-- (idempotent: hanya jalan kalau kolom belum ada)

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'hls_ready'
  ) THEN
    ALTER TABLE videos ADD COLUMN hls_ready BOOLEAN NOT NULL DEFAULT FALSE;
  END IF;

  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'hls_master'
  ) THEN
    ALTER TABLE videos ADD COLUMN hls_master TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'processing_state'
  ) THEN
    ALTER TABLE videos ADD COLUMN processing_state TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'last_error'
  ) THEN
    ALTER TABLE videos ADD COLUMN last_error TEXT;
  END IF;

  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'width'
  ) THEN
    ALTER TABLE videos ADD COLUMN width INT;
  END IF;

  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'height'
  ) THEN
    ALTER TABLE videos ADD COLUMN height INT;
  END IF;

  IF NOT EXISTS (
    SELECT 1
    FROM information_schema.columns
    WHERE table_name = 'videos' AND column_name = 'duration_sec'
  ) THEN
    ALTER TABLE videos ADD COLUMN duration_sec INT;
  END IF;

  -- index agar query hls_ready cepat
  IF NOT EXISTS (
    SELECT 1 FROM pg_indexes WHERE indexname = 'idx_videos_hls_ready'
  ) THEN
    CREATE INDEX idx_videos_hls_ready ON videos(hls_ready);
  END IF;
END $$;
