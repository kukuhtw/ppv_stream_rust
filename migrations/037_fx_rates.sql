-- 037_fx_rates.sql
-- Exchange rates and token prices used for payment conversion, plus optional
-- rupiah pricing for videos. Idempotent: safe to run more than once.
--
-- pair examples:
--   'USD/IDR'  rupiah per 1 USD            (fiat FX source)
--   'POL/USD'  USD per 1 POL               (token price oracle)
-- When is_override = TRUE the background job does not overwrite the row;
-- the admin-set rate is used until the override is removed.

CREATE TABLE IF NOT EXISTS fx_rates (
    pair         TEXT PRIMARY KEY,
    rate         DOUBLE PRECISION NOT NULL CHECK (rate > 0),
    source       TEXT NOT NULL DEFAULT 'env',
    fetched_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_override  BOOLEAN NOT NULL DEFAULT FALSE,
    updated_by   TEXT,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Audit trail for every rate change (ISO/IEC 27001 A.8.15 logging).
CREATE TABLE IF NOT EXISTS fx_rate_history (
    id           BIGSERIAL PRIMARY KEY,
    pair         TEXT NOT NULL,
    rate         DOUBLE PRECISION NOT NULL,
    source       TEXT NOT NULL,
    is_override  BOOLEAN NOT NULL DEFAULT FALSE,
    updated_by   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS fx_rate_history_pair_idx ON fx_rate_history (pair, created_at DESC);

-- Rupiah pricing: when price_currency = 'IDR' the creator's price is
-- price_idr (whole rupiah). price_cents stays the canonical USD amount used by
-- wallet / Stripe / PayPal / x402 and is re-derived from the current USD/IDR
-- rate by the FX job. Midtrans and Xendit charge price_idr exactly.
ALTER TABLE videos
    ADD COLUMN IF NOT EXISTS price_currency TEXT NOT NULL DEFAULT 'USD',
    ADD COLUMN IF NOT EXISTS price_idr      BIGINT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'videos_price_currency_check'
    ) THEN
        ALTER TABLE videos
            ADD CONSTRAINT videos_price_currency_check
            CHECK (price_currency IN ('USD', 'IDR')
                   AND (price_currency <> 'IDR' OR (price_idr IS NOT NULL AND price_idr >= 0)));
    END IF;
END $$;
