-- 017_allowlist_idx_username.sql
-- migrations/017_allowlist_idx_username.sql
-- Index untuk mempercepat cek akses berdasarkan username
CREATE INDEX IF NOT EXISTS idx_allowlist_user
  ON allowlist(username);
