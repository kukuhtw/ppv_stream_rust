-- 026_fiat_invoices.sql
-- Tracks fiat payment invoices for Stripe, PayPal, Midtrans, Xendit.
-- x402 (crypto) invoices remain in x402_invoices.
CREATE TABLE IF NOT EXISTS fiat_invoices (
  id             BIGSERIAL PRIMARY KEY,
  invoice_uid    TEXT NOT NULL UNIQUE,
  provider       TEXT NOT NULL,          -- stripe | paypal | midtrans | xendit
  provider_ref   TEXT,                   -- provider's session/order/invoice ID
  user_id        TEXT NOT NULL REFERENCES users(id),
  video_id       TEXT NOT NULL REFERENCES videos(id),
  creator_id     TEXT NOT NULL REFERENCES users(id),
  amount         BIGINT NOT NULL,        -- USD: cents (199 = $1.99); IDR: full amount (28500 = Rp28.500)
  currency       TEXT NOT NULL DEFAULT 'USD',
  status         TEXT NOT NULL DEFAULT 'pending', -- pending | paid | failed | expired | cancelled
  payment_url    TEXT,
  buyer_email    TEXT,
  meta           JSONB,
  created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  paid_at        TIMESTAMPTZ,
  disbursed_at   TIMESTAMPTZ,
  disburse_ref   TEXT
);

CREATE INDEX IF NOT EXISTS idx_fiat_uid      ON fiat_invoices(invoice_uid);
CREATE INDEX IF NOT EXISTS idx_fiat_pref     ON fiat_invoices(provider_ref);
CREATE INDEX IF NOT EXISTS idx_fiat_status   ON fiat_invoices(status);
CREATE INDEX IF NOT EXISTS idx_fiat_video    ON fiat_invoices(video_id);
CREATE INDEX IF NOT EXISTS idx_fiat_user     ON fiat_invoices(user_id);
