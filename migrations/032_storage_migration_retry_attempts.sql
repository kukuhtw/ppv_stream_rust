ALTER TABLE storage_migration_jobs
ADD COLUMN IF NOT EXISTS retry_attempts BIGINT NOT NULL DEFAULT 0;
