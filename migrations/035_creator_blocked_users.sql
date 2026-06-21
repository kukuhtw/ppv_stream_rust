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

CREATE OR REPLACE FUNCTION prevent_blocked_creator_purchase()
RETURNS TRIGGER AS $$
DECLARE
    v_owner_id TEXT;
    v_block_count BIGINT;
BEGIN
    SELECT owner_id INTO v_owner_id
    FROM videos
    WHERE id = NEW.video_id
    LIMIT 1;

    IF v_owner_id IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT COUNT(*) INTO v_block_count
    FROM creator_blocked_users
    WHERE creator_user_id = v_owner_id
      AND blocked_user_id = NEW.user_id
      AND (expires_at IS NULL OR expires_at > NOW());

    IF v_block_count > 0 THEN
        RAISE EXCEPTION 'buyer is blocked by this creator';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_prevent_blocked_creator_purchase ON purchases;
CREATE TRIGGER trg_prevent_blocked_creator_purchase
BEFORE INSERT ON purchases
FOR EACH ROW
EXECUTE FUNCTION prevent_blocked_creator_purchase();
