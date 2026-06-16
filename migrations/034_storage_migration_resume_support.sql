ALTER TABLE storage_migration_jobs
ADD COLUMN IF NOT EXISTS resumed_from_job_id TEXT REFERENCES storage_migration_jobs(id) ON DELETE SET NULL;

ALTER TABLE storage_migration_jobs
ADD COLUMN IF NOT EXISTS skipped_files BIGINT NOT NULL DEFAULT 0;
