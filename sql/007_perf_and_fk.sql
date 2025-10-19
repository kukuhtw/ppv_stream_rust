-- 007_perf_and_fk.sql

-- Percepat lookup allowlist → username
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- ===== FK: videos.owner_id → users(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'fk_videos_owner'
  ) THEN
    -- Tambahkan FK tanpa memvalidasi dulu (cepat & tidak lock lama)
    ALTER TABLE videos
      ADD CONSTRAINT fk_videos_owner
      FOREIGN KEY (owner_id) REFERENCES users(id)
      ON DELETE CASCADE
      NOT VALID;
  END IF;
END $$;

-- Validasi belakangan (aman kalau constraint baru saja dibuat)
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'fk_videos_owner' AND NOT convalidated
  ) THEN
    ALTER TABLE videos VALIDATE CONSTRAINT fk_videos_owner;
  END IF;
END $$;

-- ===== FK: purchases.video_id → videos(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'fk_purchases_video'
  ) THEN
    ALTER TABLE purchases
      ADD CONSTRAINT fk_purchases_video
      FOREIGN KEY (video_id) REFERENCES videos(id)
      ON DELETE CASCADE
      NOT VALID;
  END IF;
END $$;

DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'fk_purchases_video' AND NOT convalidated
  ) THEN
    ALTER TABLE purchases VALIDATE CONSTRAINT fk_purchases_video;
  END IF;
END $$;
