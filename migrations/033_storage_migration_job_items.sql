CREATE TABLE IF NOT EXISTS storage_migration_job_items (
  id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES storage_migration_jobs(id) ON DELETE CASCADE,
  scope TEXT NOT NULL,
  source_path TEXT NOT NULL,
  object_key TEXT NOT NULL,
  status TEXT NOT NULL,
  retry_attempts BIGINT NOT NULL DEFAULT 0,
  error_message TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_storage_migration_job_items_job_created_at
  ON storage_migration_job_items (job_id, created_at DESC);
