CREATE TABLE IF NOT EXISTS creator_blocked_users (
    id BIGSERIAL PRIMARY KEY,
    creator_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ban_type TEXT NOT NULL DEFAULT 'soft',
    reason TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT creator_blocked_users_no_self_block CHECK (creator_user_id <> blocked_user_id),
    CONSTRAINT creator_blocked_users_ban_type_check CHECK (ban_type IN ('soft', 'hard')),
    CONSTRAINT creator_blocked_users_unique_pair UNIQUE (creator_user_id, blocked_user_id)
);

CREATE INDEX IF NOT EXISTS idx_creator_blocked_users_creator
    ON creator_blocked_users (creator_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_creator_blocked_users_blocked
    ON creator_blocked_users (blocked_user_id);
