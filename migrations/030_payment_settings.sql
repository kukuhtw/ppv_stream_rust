-- Admin-configurable payment method toggles.
-- Secrets stay in .env; this table only stores which methods are enabled.

CREATE TABLE IF NOT EXISTS payment_settings (
  id BOOLEAN PRIMARY KEY DEFAULT TRUE,
  wallet_payment_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  wallet_transfer_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  paypal_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  stripe_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  xendit_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  midtrans_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  x402_enabled BOOLEAN NOT NULL DEFAULT TRUE,
  default_provider TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT payment_settings_singleton CHECK (id = TRUE)
);

INSERT INTO payment_settings (id)
VALUES (TRUE)
ON CONFLICT (id) DO NOTHING;
