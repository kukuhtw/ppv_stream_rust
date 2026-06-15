CREATE TABLE IF NOT EXISTS playback_sessions (
    session_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    video_id TEXT NOT NULL,
    session_dir TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'starting',
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_playback_sessions_expires_at
    ON playback_sessions (expires_at);

CREATE INDEX IF NOT EXISTS idx_playback_sessions_user_id
    ON playback_sessions (user_id);
