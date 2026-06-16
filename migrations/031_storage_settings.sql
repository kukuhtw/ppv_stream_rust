CREATE TABLE IF NOT EXISTS storage_settings (
  id BOOLEAN PRIMARY KEY DEFAULT TRUE,
  backend TEXT NOT NULL DEFAULT 'local',
  bucket TEXT NOT NULL DEFAULT '',
  region TEXT NOT NULL DEFAULT 'us-east-1',
  access_key TEXT NOT NULL DEFAULT '',
  secret_key TEXT NOT NULL DEFAULT '',
  endpoint TEXT NOT NULL DEFAULT '',
  public_url TEXT NOT NULL DEFAULT '',
  path_style BOOLEAN NOT NULL DEFAULT FALSE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT storage_settings_singleton CHECK (id = TRUE)
);

INSERT INTO storage_settings (id)
VALUES (TRUE)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS storage_migration_jobs (
  id TEXT PRIMARY KEY,
  status TEXT NOT NULL DEFAULT 'pending',
  backend TEXT NOT NULL,
  bucket TEXT NOT NULL DEFAULT '',
  endpoint TEXT NOT NULL DEFAULT '',
  include_uploads BOOLEAN NOT NULL DEFAULT TRUE,
  include_media BOOLEAN NOT NULL DEFAULT TRUE,
  total_files BIGINT NOT NULL DEFAULT 0,
  copied_files BIGINT NOT NULL DEFAULT 0,
  failed_files BIGINT NOT NULL DEFAULT 0,
  last_error TEXT,
  started_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  started_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_storage_migration_jobs_created_at
  ON storage_migration_jobs (created_at DESC);
