-- migrations/016_purchases_fk_video.sql
-- Pastikan FK purchases(video_id) -> videos(id)
-- Catatan: ADD CONSTRAINT tidak punya IF NOT EXISTS, jadi pakai DO $$...$$

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM   pg_constraint
    WHERE  conname = 'purchases_video_fk'
  ) THEN
    -- Tambahkan FK sebagai NOT VALID agar lock minimal
    ALTER TABLE purchases
      ADD CONSTRAINT purchases_video_fk
      FOREIGN KEY (video_id) REFERENCES videos(id)
      ON UPDATE CASCADE
      ON DELETE RESTRICT
      NOT VALID;

    -- Validasi setelahnya (masih dalam blok DO)
    ALTER TABLE purchases
      VALIDATE CONSTRAINT purchases_video_fk;
  END IF;
END
$$;
