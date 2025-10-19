-- 005_allowlist.sql
CREATE TABLE IF NOT EXISTS allowlist (
  video_id TEXT NOT NULL,
  username TEXT NOT NULL,
  UNIQUE(video_id, username)
);
CREATE INDEX IF NOT EXISTS idx_allowlist_video ON allowlist(video_id);
