-- migrations/027_smtp_settings.sql
-- Single-row table for SMTP configuration (editable from admin dashboard).
-- Always maintained as row id=1 via upsert.
CREATE TABLE IF NOT EXISTS smtp_settings (
  id          SERIAL PRIMARY KEY,
  host        TEXT NOT NULL DEFAULT '',
  port        INTEGER NOT NULL DEFAULT 587,
  username    TEXT NOT NULL DEFAULT '',
  password    TEXT NOT NULL DEFAULT '',
  from_email  TEXT NOT NULL DEFAULT '',
  from_name   TEXT NOT NULL DEFAULT 'PPV Stream',
  use_tls     BOOLEAN NOT NULL DEFAULT true,
  enabled     BOOLEAN NOT NULL DEFAULT false,
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Seed the single settings row so the app always finds id=1
INSERT INTO smtp_settings (id) VALUES (1) ON CONFLICT DO NOTHING;
