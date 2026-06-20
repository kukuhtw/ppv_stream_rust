-- Index-only federation foundation.
-- Remote media files, HLS manifests, HLS segments, transcoded outputs,
-- playback sessions, and protected content must never be stored locally.

CREATE TABLE IF NOT EXISTS federation_instances (
    id UUID PRIMARY KEY,
    domain TEXT NOT NULL UNIQUE,
    base_url TEXT NOT NULL,
    software_name TEXT,
    software_version TEXT,
    shared_inbox_url TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    trust_level TEXT NOT NULL DEFAULT 'unknown',
    last_seen_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (status IN ('active', 'blocked', 'silenced', 'suspended', 'unreachable'))
);

CREATE TABLE IF NOT EXISTS federation_actors (
    id UUID PRIMARY KEY,
    local_user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    actor_uri TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL,
    domain TEXT NOT NULL,
    display_name TEXT,
    inbox_url TEXT NOT NULL,
    outbox_url TEXT,
    followers_url TEXT,
    following_url TEXT,
    shared_inbox_url TEXT,
    public_key_id TEXT,
    public_key_pem TEXT,
    private_key_encrypted TEXT,
    avatar_url TEXT,
    profile_url TEXT,
    is_local BOOLEAN NOT NULL DEFAULT FALSE,
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    fetched_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (is_local OR private_key_encrypted IS NULL)
);

CREATE INDEX IF NOT EXISTS idx_federation_actors_handle
    ON federation_actors (username, domain);
CREATE INDEX IF NOT EXISTS idx_federation_actors_domain
    ON federation_actors (domain);

CREATE TABLE IF NOT EXISTS federation_follows (
    id UUID PRIMARY KEY,
    follower_actor_id UUID NOT NULL REFERENCES federation_actors(id) ON DELETE CASCADE,
    following_actor_id UUID NOT NULL REFERENCES federation_actors(id) ON DELETE CASCADE,
    activity_uri TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (follower_actor_id, following_actor_id),
    CHECK (status IN ('pending', 'accepted', 'rejected', 'cancelled'))
);

CREATE TABLE IF NOT EXISTS federation_activities (
    id UUID PRIMARY KEY,
    activity_uri TEXT NOT NULL UNIQUE,
    activity_type TEXT NOT NULL,
    actor_uri TEXT NOT NULL,
    object_uri TEXT,
    direction TEXT NOT NULL,
    payload JSONB NOT NULL,
    processing_status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    received_at TIMESTAMPTZ,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (direction IN ('inbound', 'outbound')),
    CHECK (processing_status IN ('pending', 'processing', 'processed', 'failed', 'ignored'))
);

CREATE INDEX IF NOT EXISTS idx_federation_activities_processing
    ON federation_activities (processing_status, created_at);

CREATE TABLE IF NOT EXISTS federation_delivery_jobs (
    id UUID PRIMARY KEY,
    activity_id UUID NOT NULL REFERENCES federation_activities(id) ON DELETE CASCADE,
    target_inbox_url TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 10,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL DEFAULT 'queued',
    last_http_status INTEGER,
    last_error TEXT,
    delivered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (attempt_count >= 0),
    CHECK (max_attempts > 0),
    CHECK (status IN ('queued', 'processing', 'delivered', 'failed', 'cancelled'))
);

CREATE INDEX IF NOT EXISTS idx_federation_delivery_due
    ON federation_delivery_jobs (status, next_attempt_at);

CREATE TABLE IF NOT EXISTS remote_video_catalog (
    id UUID PRIMARY KEY,
    object_uri TEXT NOT NULL UNIQUE,
    origin_actor_uri TEXT NOT NULL,
    origin_domain TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    thumbnail_url TEXT,
    trailer_url TEXT,
    canonical_url TEXT NOT NULL,
    checkout_url TEXT,
    price_amount NUMERIC,
    price_currency TEXT,
    content_rating TEXT,
    availability_status TEXT NOT NULL DEFAULT 'available',
    published_at TIMESTAMPTZ,
    raw_object JSONB NOT NULL,
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (availability_status IN ('available', 'unavailable', 'deleted', 'origin_unreachable'))
);

CREATE INDEX IF NOT EXISTS idx_remote_video_catalog_origin
    ON remote_video_catalog (origin_domain, is_deleted);
CREATE INDEX IF NOT EXISTS idx_remote_video_catalog_published
    ON remote_video_catalog (published_at DESC);

CREATE TABLE IF NOT EXISTS federation_domain_rules (
    id UUID PRIMARY KEY,
    domain TEXT NOT NULL UNIQUE,
    action TEXT NOT NULL,
    reason TEXT,
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (action IN ('allow', 'silence', 'reject_media', 'suspend', 'block'))
);

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS actor_uri TEXT,
    ADD COLUMN IF NOT EXISTS federation_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS discoverable BOOLEAN NOT NULL DEFAULT TRUE;

CREATE UNIQUE INDEX IF NOT EXISTS users_actor_uri_uq
    ON users (actor_uri)
    WHERE actor_uri IS NOT NULL;

ALTER TABLE videos
    ADD COLUMN IF NOT EXISTS object_uri TEXT,
    ADD COLUMN IF NOT EXISTS federation_visibility TEXT NOT NULL DEFAULT 'local_only',
    ADD COLUMN IF NOT EXISTS federated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS federation_updated_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS videos_object_uri_uq
    ON videos (object_uri)
    WHERE object_uri IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'videos_federation_visibility_check'
    ) THEN
        ALTER TABLE videos
            ADD CONSTRAINT videos_federation_visibility_check
            CHECK (federation_visibility IN ('public', 'unlisted', 'followers', 'local_only', 'private'));
    END IF;
END $$;
